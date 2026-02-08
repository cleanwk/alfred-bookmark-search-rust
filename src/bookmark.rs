use serde::{Deserialize, Serialize};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

/// Chrome书签项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChromeBookmark {
    pub id: String,
    pub name: String,
    pub url: String,
    pub date_added: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folder_path: Option<String>,
    /// 预计算的小写名称，用于加速搜索
    #[serde(skip)]
    pub name_lower: String,
    /// 预计算的小写URL
    #[serde(skip)]
    pub url_lower: String,
    /// 预计算的小写文件夹路径
    #[serde(skip)]
    pub folder_path_lower: Option<String>,
}

/// Chrome书签文件的根结构
#[derive(Debug, Deserialize)]
pub struct ChromeBookmarks {
    pub roots: BookmarkRoots,
}

#[derive(Debug, Deserialize)]
pub struct BookmarkRoots {
    pub bookmark_bar: BookmarkNode,
    pub other: BookmarkNode,
    #[serde(default)]
    pub synced: Option<BookmarkNode>,
}

#[derive(Debug, Deserialize)]
pub struct BookmarkNode {
    #[serde(rename = "type")]
    pub node_type: String,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub date_added: Option<String>,
    #[serde(default)]
    pub children: Vec<BookmarkNode>,
}

#[derive(Clone, Copy)]
struct BrowserSource {
    key: &'static str,
    aliases: &'static [&'static str],
    roots: &'static [&'static str],
}

const BROWSER_SOURCES: &[BrowserSource] = &[
    BrowserSource {
        key: "chrome",
        aliases: &["google-chrome", "google"],
        roots: &[
            "Google/Chrome",
            "Google/Chrome Beta",
            "Google/Chrome Dev",
            "Google/Chrome Canary",
        ],
    },
    BrowserSource {
        key: "brave",
        aliases: &["brave-browser"],
        roots: &[
            "BraveSoftware/Brave-Browser",
            "BraveSoftware/Brave-Browser-Beta",
            "BraveSoftware/Brave-Browser-Nightly",
        ],
    },
    BrowserSource {
        key: "edge",
        aliases: &["microsoft-edge", "msedge"],
        roots: &[
            "Microsoft Edge",
            "Microsoft Edge Beta",
            "Microsoft Edge Dev",
            "Microsoft Edge Canary",
        ],
    },
    BrowserSource {
        key: "chromium",
        aliases: &[],
        roots: &["Chromium"],
    },
    BrowserSource {
        key: "vivaldi",
        aliases: &[],
        roots: &["Vivaldi"],
    },
    BrowserSource {
        key: "arc",
        aliases: &[],
        roots: &["Arc", "The Browser Company/Arc"],
    },
    BrowserSource {
        key: "dia",
        aliases: &[],
        roots: &["Dia", "The Browser Company/Dia"],
    },
    BrowserSource {
        key: "opera",
        aliases: &["opera-stable"],
        roots: &["Opera", "com.operasoftware.Opera"],
    },
    BrowserSource {
        key: "opera-developer",
        aliases: &["opera-dev"],
        roots: &["com.operasoftware.OperaDeveloper"],
    },
    BrowserSource {
        key: "opera-next",
        aliases: &["opera-beta"],
        roots: &["com.operasoftware.OperaNext"],
    },
    BrowserSource {
        key: "opera-gx",
        aliases: &["operagx"],
        roots: &["com.operasoftware.OperaGX"],
    },
    BrowserSource {
        key: "sidekick",
        aliases: &[],
        roots: &["Sidekick"],
    },
];

impl ChromeBookmarks {
    /// 从文件读取Chrome书签
    pub fn from_file(path: PathBuf) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let bookmarks: ChromeBookmarks = serde_json::from_str(&content)?;
        Ok(bookmarks)
    }

    /// 提取所有书签（平铺结构）
    pub fn extract_all_bookmarks(&self) -> Vec<ChromeBookmark> {
        let mut bookmarks = Vec::new();

        // 处理书签栏
        self.extract_from_node(&self.roots.bookmark_bar, "书签栏", &mut bookmarks);

        // 处理其他书签
        self.extract_from_node(&self.roots.other, "其他书签", &mut bookmarks);

        // 处理同步书签
        if let Some(ref synced) = self.roots.synced {
            self.extract_from_node(synced, "同步书签", &mut bookmarks);
        }

        bookmarks
    }

    fn extract_from_node(
        &self,
        node: &BookmarkNode,
        folder_path: &str,
        bookmarks: &mut Vec<ChromeBookmark>,
    ) {
        if node.node_type == "url" {
            if let (Some(url), Some(date_added)) = (&node.url, &node.date_added) {
                let folder_path_str = folder_path.to_string();
                bookmarks.push(ChromeBookmark {
                    name_lower: node.name.to_lowercase(),
                    url_lower: url.to_lowercase(),
                    folder_path_lower: Some(folder_path_str.to_lowercase()),
                    id: node.id.clone(),
                    name: node.name.clone(),
                    url: url.clone(),
                    date_added: date_added.clone(),
                    folder_path: Some(folder_path_str),
                });
            }
        } else if node.node_type == "folder" {
            let new_path = format!("{}/{}", folder_path, node.name);
            for child in &node.children {
                self.extract_from_node(child, &new_path, bookmarks);
            }
        }
    }
}

/// 使用缓存获取受支持浏览器书签路径，减少每次调用的目录扫描成本
pub fn get_chrome_bookmarks_path_cached(cache_dir: &Path) -> Option<PathBuf> {
    if let Some(configured) = resolve_configured_bookmarks_path() {
        return Some(configured);
    }

    let browser_key = resolve_configured_browser_key();
    dirs::home_dir().and_then(|home| {
        get_chrome_bookmarks_path_cached_from_home_for_browser(
            &home,
            cache_dir,
            browser_key.as_deref(),
        )
    })
}

fn get_chrome_bookmarks_path_from_home_for_browser(
    home: &Path,
    browser_key: Option<&str>,
) -> Option<PathBuf> {
    let app_support_dir = home.join("Library/Application Support");
    let mut candidates = Vec::new();
    collect_bookmark_candidates(&app_support_dir, browser_key, &mut candidates);

    select_latest_bookmarks(candidates)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BookmarksSourceCache {
    path: String,
    modified_nanos: u128,
    size: u64,
}

fn resolve_configured_bookmarks_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("ALFRED_CHROME_BOOKMARKS_PATH") {
        let configured = PathBuf::from(path);
        if configured.exists() {
            return Some(configured);
        }
    }
    None
}

fn resolve_configured_browser_key() -> Option<String> {
    let raw = std::env::var("ALFRED_CHROME_BOOKMARKS_BROWSER").ok()?;
    let normalized = normalize_browser_identifier(&raw);
    if normalized.is_empty() || normalized == "all" {
        return None;
    }

    match find_browser_source(&normalized) {
        Some(source) => Some(source.key.to_string()),
        None => Some(normalized),
    }
}

fn get_chrome_bookmarks_path_cached_from_home_for_browser(
    home: &Path,
    cache_dir: &Path,
    browser_key: Option<&str>,
) -> Option<PathBuf> {
    let cache_file = bookmarks_source_cache_file(cache_dir, browser_key);
    if let Some(cached_path) = load_cached_bookmarks_source_path(&cache_file) {
        return Some(cached_path);
    }

    let discovered = get_chrome_bookmarks_path_from_home_for_browser(home, browser_key)?;
    let _ = save_cached_bookmarks_source_path(&cache_file, &discovered);
    Some(discovered)
}

fn bookmarks_source_cache_file(cache_dir: &Path, browser_key: Option<&str>) -> PathBuf {
    match browser_key {
        Some(key) => {
            let safe_key = key
                .chars()
                .map(|ch| {
                    if ch.is_ascii_alphanumeric() || ch == '-' {
                        ch
                    } else {
                        '_'
                    }
                })
                .collect::<String>();
            cache_dir.join(format!("bookmarks_source_path.{}.json", safe_key))
        }
        None => cache_dir.join("bookmarks_source_path.json"),
    }
}

fn normalize_browser_identifier(raw: &str) -> String {
    raw.trim()
        .chars()
        .map(|ch| match ch {
            '_' | ' ' => '-',
            _ => ch.to_ascii_lowercase(),
        })
        .collect()
}

fn find_browser_source(identifier: &str) -> Option<&'static BrowserSource> {
    BROWSER_SOURCES
        .iter()
        .find(|source| source.key == identifier || source.aliases.contains(&identifier))
}

fn collect_bookmark_candidates(
    app_support_dir: &Path,
    browser_key: Option<&str>,
    candidates: &mut Vec<PathBuf>,
) {
    if let Some(key) = browser_key {
        let Some(source) = find_browser_source(key) else {
            return;
        };
        for browser_root in source.roots {
            collect_bookmarks_from_browser_root(&app_support_dir.join(browser_root), candidates);
        }
        return;
    }

    for source in BROWSER_SOURCES {
        for browser_root in source.roots {
            collect_bookmarks_from_browser_root(&app_support_dir.join(browser_root), candidates);
        }
    }
}

fn load_cached_bookmarks_source_path(cache_file: &Path) -> Option<PathBuf> {
    let data = std::fs::read(cache_file).ok()?;
    let cache = serde_json::from_slice::<BookmarksSourceCache>(&data).ok()?;
    let path = PathBuf::from(cache.path);
    let metadata = std::fs::metadata(&path).ok()?;
    let modified_nanos = metadata
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();

    if modified_nanos == cache.modified_nanos && metadata.len() == cache.size {
        Some(path)
    } else {
        None
    }
}

fn save_cached_bookmarks_source_path(cache_file: &Path, path: &Path) -> std::io::Result<()> {
    let metadata = std::fs::metadata(path)?;
    let modified_nanos = metadata
        .modified()
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    let cache = BookmarksSourceCache {
        path: path.to_string_lossy().to_string(),
        modified_nanos,
        size: metadata.len(),
    };
    let bytes = serde_json::to_vec(&cache).map_err(|err| std::io::Error::other(err.to_string()))?;
    write_atomic(cache_file, &bytes)
}

fn collect_bookmarks_from_browser_root(root: &Path, candidates: &mut Vec<PathBuf>) {
    if !root.exists() {
        return;
    }

    let root_bookmarks = root.join("Bookmarks");
    if root_bookmarks.exists() {
        candidates.push(root_bookmarks);
    }

    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }

        let profile_name = entry.file_name();
        let profile_name = profile_name.to_string_lossy();
        if !is_chromium_profile_dir(&profile_name) {
            continue;
        }

        let profile_bookmarks = entry.path().join("Bookmarks");
        if profile_bookmarks.exists() {
            candidates.push(profile_bookmarks);
        }
    }
}

fn is_chromium_profile_dir(name: &str) -> bool {
    name == "Default"
        || name == "Guest Profile"
        || name == "System Profile"
        || name.starts_with("Profile ")
        || name.starts_with("Person ")
}

fn select_latest_bookmarks(mut candidates: Vec<PathBuf>) -> Option<PathBuf> {
    if candidates.is_empty() {
        return None;
    }

    let mut selected: Option<(std::time::SystemTime, u64, PathBuf)> = None;

    for path in candidates.drain(..) {
        let metadata = match std::fs::metadata(&path) {
            Ok(meta) => meta,
            Err(_) => continue,
        };
        let modified = metadata
            .modified()
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        let size = metadata.len();

        match selected {
            None => selected = Some((modified, size, path)),
            Some((best_time, best_size, _)) => {
                if modified > best_time || (modified == best_time && size > best_size) {
                    selected = Some((modified, size, path));
                }
            }
        }
    }

    selected.map(|(_, _, path)| path)
}

pub fn compute_bookmarks_fingerprint(
    bookmarks_path: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    let metadata = std::fs::metadata(bookmarks_path)?;
    let mtime_nanos = metadata
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let canonical = bookmarks_path
        .canonicalize()
        .unwrap_or_else(|_| bookmarks_path.to_path_buf());
    Ok(format!(
        "{}-{}-{}",
        mtime_nanos,
        metadata.len(),
        canonical.to_string_lossy()
    ))
}

/// 带mtime缓存的书签加载器
/// 将解析后的书签序列化到本地缓存文件，只有Chrome书签文件变化时才重新解析
pub struct BookmarkCache {
    cache_path: PathBuf,
    mtime_path: PathBuf,
}

impl BookmarkCache {
    pub fn new(data_dir: &Path) -> Self {
        BookmarkCache {
            cache_path: data_dir.join("bookmarks_cache.json"),
            mtime_path: data_dir.join("bookmarks_mtime"),
        }
    }

    pub fn invalidate(&self) {
        let _ = std::fs::remove_file(&self.cache_path);
        let _ = std::fs::remove_file(&self.mtime_path);
    }

    /// 加载书签，使用缓存（如果Chrome书签文件未变化）
    pub fn load(
        &self,
        bookmarks_path: &Path,
    ) -> Result<Vec<ChromeBookmark>, Box<dyn std::error::Error>> {
        let source_fingerprint = Self::fingerprint(bookmarks_path)?;

        // 检查缓存是否仍然有效
        if let Ok(cached_fingerprint) = std::fs::read_to_string(&self.mtime_path) {
            if cached_fingerprint.trim() == source_fingerprint {
                if let Some(bookmarks) = self.load_cached() {
                    return Ok(bookmarks);
                }
            }
        }

        // 缓存失效，重新解析
        let chrome_bookmarks = match ChromeBookmarks::from_file(bookmarks_path.to_path_buf()) {
            Ok(parsed) => parsed,
            Err(err) => {
                if let Some(bookmarks) = self.load_cached() {
                    return Ok(bookmarks);
                }
                return Err(err);
            }
        };
        let bookmarks = chrome_bookmarks.extract_all_bookmarks();

        // 写入缓存（忽略写入失败，不影响功能）
        if let Ok(json) = serde_json::to_vec(&bookmarks) {
            let _ = write_atomic(&self.cache_path, &json);
        }
        let _ = write_atomic(&self.mtime_path, source_fingerprint.as_bytes());

        Ok(bookmarks)
    }

    fn load_cached(&self) -> Option<Vec<ChromeBookmark>> {
        let cached_data = std::fs::read(&self.cache_path).ok()?;
        let bookmarks = serde_json::from_slice::<Vec<ChromeBookmark>>(&cached_data).ok()?;

        let bookmarks = bookmarks
            .into_iter()
            .map(|mut b| {
                b.name_lower = b.name.to_lowercase();
                b.url_lower = b.url.to_lowercase();
                b.folder_path_lower = b.folder_path.as_ref().map(|p| p.to_lowercase());
                b
            })
            .collect();

        Some(bookmarks)
    }

    fn fingerprint(bookmarks_path: &Path) -> Result<String, Box<dyn std::error::Error>> {
        compute_bookmarks_fingerprint(bookmarks_path)
    }
}

fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp_path = path.with_extension("tmp");
    if let Ok(file) = std::fs::File::create(&tmp_path) {
        let mut writer = BufWriter::new(file);
        writer.write_all(data)?;
        writer.flush()?;
        let _ = writer.get_ref().sync_all();
        std::fs::rename(tmp_path, path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn write_bookmarks(path: &Path, include_other: bool) {
        let other_section = if include_other {
            r#"
    "other": {
      "type": "folder",
      "id": "2",
      "name": "Other Bookmarks",
      "children": [
        {
          "type": "url",
          "id": "20",
          "name": "Other",
          "url": "https://other.com",
          "date_added": "3"
        }
      ]
    }"#
        } else {
            r#"
    "other": {
      "type": "folder",
      "id": "2",
      "name": "Other Bookmarks",
      "children": []
    }"#
        };

        let json = format!(
            r#"{{
  "roots": {{
    "bookmark_bar": {{
      "type": "folder",
      "id": "1",
      "name": "Bookmark Bar",
      "children": [
        {{
          "type": "url",
          "id": "10",
          "name": "Rust",
          "url": "https://rust-lang.org",
          "date_added": "1"
        }},
        {{
          "type": "folder",
          "id": "11",
          "name": "Sub",
          "children": [
            {{
              "type": "url",
              "id": "12",
              "name": "Example",
              "url": "https://example.com",
              "date_added": "2"
            }}
          ]
        }}
      ]
    }},{}
  }}
}}"#,
            other_section
        );
        fs::write(path, json).expect("write bookmarks");
    }

    #[test]
    fn extract_all_bookmarks_builds_paths_and_lowercase() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("Bookmarks");
        write_bookmarks(&path, true);

        let parsed = ChromeBookmarks::from_file(path).expect("parse bookmarks");
        let bookmarks = parsed.extract_all_bookmarks();
        assert_eq!(bookmarks.len(), 3);

        let rust = bookmarks.iter().find(|b| b.id == "10").unwrap();
        assert_eq!(rust.name_lower, "rust");
        assert_eq!(rust.url_lower, "https://rust-lang.org");

        let nested = bookmarks.iter().find(|b| b.id == "12").unwrap();
        assert_eq!(
            nested.folder_path.as_deref(),
            Some("书签栏/Bookmark Bar/Sub")
        );
        assert_eq!(
            nested.folder_path_lower.as_deref(),
            Some("书签栏/bookmark bar/sub")
        );
    }

    #[test]
    fn get_chrome_bookmarks_path_from_home_selects_latest_profile() {
        let dir = tempdir().expect("tempdir");
        let home = dir.path();

        let default_path = home.join("Library/Application Support/Google/Chrome/Default");
        fs::create_dir_all(&default_path).expect("create default");
        fs::write(default_path.join("Bookmarks"), "{}").expect("write default");

        let profile_path = home.join("Library/Application Support/Google/Chrome/Profile 1");
        fs::create_dir_all(&profile_path).expect("create profile");
        fs::write(profile_path.join("Bookmarks"), "{\"bigger\":true}").expect("write profile");

        let found_profile =
            get_chrome_bookmarks_path_from_home_for_browser(home, None).expect("find latest");
        assert!(found_profile.ends_with("Profile 1/Bookmarks"));

        fs::remove_file(profile_path.join("Bookmarks")).expect("remove profile");
        let found_default =
            get_chrome_bookmarks_path_from_home_for_browser(home, None).expect("find default");
        assert!(found_default.ends_with("Default/Bookmarks"));
    }

    #[test]
    fn get_chrome_bookmarks_path_supports_arc_and_dia_roots() {
        let dir = tempdir().expect("tempdir");
        let home = dir.path();

        let arc_profile = home.join("Library/Application Support/Arc/Default");
        fs::create_dir_all(&arc_profile).expect("create arc default");
        fs::write(arc_profile.join("Bookmarks"), "{}").expect("write arc bookmarks");

        let dia_profile =
            home.join("Library/Application Support/The Browser Company/Dia/Profile 2");
        fs::create_dir_all(&dia_profile).expect("create dia profile");
        fs::write(dia_profile.join("Bookmarks"), "{\"size\":1}").expect("write dia bookmarks");

        let found =
            get_chrome_bookmarks_path_from_home_for_browser(home, None).expect("find bookmarks");
        let found_str = found.to_string_lossy();
        assert!(found_str.contains("/Arc/") || found_str.contains("/Dia/"));
    }

    #[test]
    fn get_chrome_bookmarks_path_from_home_respects_browser_filter() {
        let dir = tempdir().expect("tempdir");
        let home = dir.path();

        let chrome_profile = home.join("Library/Application Support/Google/Chrome/Default");
        fs::create_dir_all(&chrome_profile).expect("create chrome profile");
        fs::write(chrome_profile.join("Bookmarks"), "{}").expect("write chrome bookmarks");

        let dia_profile =
            home.join("Library/Application Support/The Browser Company/Dia/Profile 2");
        fs::create_dir_all(&dia_profile).expect("create dia profile");
        fs::write(dia_profile.join("Bookmarks"), "{\"bigger\":true}").expect("write dia bookmarks");

        let chrome_only =
            get_chrome_bookmarks_path_from_home_for_browser(home, Some("chrome")).expect("chrome");
        assert!(chrome_only.ends_with("Google/Chrome/Default/Bookmarks"));

        let dia_only =
            get_chrome_bookmarks_path_from_home_for_browser(home, Some("dia")).expect("dia");
        assert!(dia_only.ends_with("The Browser Company/Dia/Profile 2/Bookmarks"));

        assert!(get_chrome_bookmarks_path_from_home_for_browser(home, Some("unknown")).is_none());
    }

    #[test]
    fn cached_bookmarks_path_uses_saved_path_when_valid() {
        let dir = tempdir().expect("tempdir");
        let home = dir.path();
        let cache_dir = home.join("cache");
        fs::create_dir_all(&cache_dir).expect("create cache");

        let default_path = home.join("Library/Application Support/Google/Chrome/Default");
        fs::create_dir_all(&default_path).expect("create default");
        fs::write(default_path.join("Bookmarks"), "{}").expect("write default");

        let first = get_chrome_bookmarks_path_cached_from_home_for_browser(home, &cache_dir, None)
            .expect("first resolve");
        let second = get_chrome_bookmarks_path_cached_from_home_for_browser(home, &cache_dir, None)
            .expect("second resolve");

        assert_eq!(first, second);
        assert!(cache_dir.join("bookmarks_source_path.json").exists());
    }

    #[test]
    fn cached_bookmarks_path_falls_back_to_rescan_when_cache_stale() {
        let dir = tempdir().expect("tempdir");
        let home = dir.path();
        let cache_dir = home.join("cache");
        fs::create_dir_all(&cache_dir).expect("create cache");

        let default_path = home.join("Library/Application Support/Google/Chrome/Default");
        fs::create_dir_all(&default_path).expect("create default");
        fs::write(default_path.join("Bookmarks"), "{}").expect("write default");

        let profile_path = home.join("Library/Application Support/Google/Chrome/Profile 1");
        fs::create_dir_all(&profile_path).expect("create profile");
        fs::write(profile_path.join("Bookmarks"), "{\"bigger\":true}").expect("write profile");

        let first = get_chrome_bookmarks_path_cached_from_home_for_browser(home, &cache_dir, None)
            .expect("first resolve");
        assert!(first.ends_with("Profile 1/Bookmarks"));

        fs::remove_file(profile_path.join("Bookmarks")).expect("remove profile bookmarks");

        let second = get_chrome_bookmarks_path_cached_from_home_for_browser(home, &cache_dir, None)
            .expect("second resolve");
        assert!(second.ends_with("Default/Bookmarks"));
    }

    #[test]
    fn cached_bookmarks_path_isolated_by_browser_filter() {
        let dir = tempdir().expect("tempdir");
        let home = dir.path();
        let cache_dir = home.join("cache");
        fs::create_dir_all(&cache_dir).expect("create cache");

        let chrome_path = home.join("Library/Application Support/Google/Chrome/Default");
        fs::create_dir_all(&chrome_path).expect("create chrome");
        fs::write(chrome_path.join("Bookmarks"), "{}").expect("write chrome");

        let dia_path = home.join("Library/Application Support/The Browser Company/Dia/Default");
        fs::create_dir_all(&dia_path).expect("create dia");
        fs::write(dia_path.join("Bookmarks"), "{\"dia\":1}").expect("write dia");

        let chrome = get_chrome_bookmarks_path_cached_from_home_for_browser(
            home,
            &cache_dir,
            Some("chrome"),
        )
        .expect("chrome resolve");
        let dia =
            get_chrome_bookmarks_path_cached_from_home_for_browser(home, &cache_dir, Some("dia"))
                .expect("dia resolve");

        assert!(chrome.ends_with("Google/Chrome/Default/Bookmarks"));
        assert!(dia.ends_with("The Browser Company/Dia/Default/Bookmarks"));
        assert!(cache_dir.join("bookmarks_source_path.chrome.json").exists());
        assert!(cache_dir.join("bookmarks_source_path.dia.json").exists());
    }

    #[test]
    fn bookmark_cache_invalidates_on_change_and_can_be_cleared() {
        let dir = tempdir().expect("tempdir");
        let data_dir = dir.path().to_path_buf();
        let bookmarks_path = dir.path().join("Bookmarks");

        write_bookmarks(&bookmarks_path, true);
        let cache = BookmarkCache::new(&data_dir);
        let first = cache.load(&bookmarks_path).expect("first load");
        assert_eq!(first.len(), 3);

        write_bookmarks(&bookmarks_path, false);

        let second = cache.load(&bookmarks_path).expect("second load");
        assert_eq!(second.len(), 2);

        let cache_file = data_dir.join("bookmarks_cache.json");
        let mtime_file = data_dir.join("bookmarks_mtime");
        assert!(cache_file.exists());
        assert!(mtime_file.exists());

        cache.invalidate();
        assert!(!cache_file.exists());
        assert!(!mtime_file.exists());
    }

    #[test]
    fn bookmark_cache_falls_back_when_parse_fails() {
        let dir = tempdir().expect("tempdir");
        let data_dir = dir.path().to_path_buf();
        let bookmarks_path = dir.path().join("Bookmarks");

        write_bookmarks(&bookmarks_path, true);
        let cache = BookmarkCache::new(&data_dir);
        let first = cache.load(&bookmarks_path).expect("first load");
        assert_eq!(first.len(), 3);

        fs::write(&bookmarks_path, "{ invalid json").expect("write invalid");

        let fallback = cache.load(&bookmarks_path).expect("fallback load");
        assert_eq!(fallback.len(), 3);
    }
}
