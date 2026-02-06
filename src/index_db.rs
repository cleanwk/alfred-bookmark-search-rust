use crate::bookmark::ChromeBookmark;
use crate::searcher::folder_filter_to_like_pattern;
use rusqlite::{params, params_from_iter, Connection, Result, ToSql};
use std::path::PathBuf;
use std::time::Duration;

pub struct BookmarkIndex {
    conn: Connection,
    fts_enabled: bool,
}

impl BookmarkIndex {
    pub fn new(db_path: PathBuf) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        let _ = conn.busy_timeout(Duration::from_millis(500));

        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA temp_store = MEMORY;
             PRAGMA cache_size = -4000;
             PRAGMA mmap_size = 268435456;",
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS bookmarks (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                url TEXT NOT NULL,
                date_added TEXT NOT NULL,
                folder_path TEXT
            )",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_bookmarks_name ON bookmarks(name)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_bookmarks_url ON bookmarks(url)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_bookmarks_folder_path ON bookmarks(folder_path)",
            [],
        )?;

        let fts_enabled = conn
            .execute(
                "CREATE VIRTUAL TABLE IF NOT EXISTS bookmarks_fts USING fts5(
                    bookmark_id UNINDEXED,
                    name,
                    url,
                    folder_path,
                    tokenize = 'unicode61'
                )",
                [],
            )
            .is_ok();

        Ok(Self { conn, fts_enabled })
    }

    pub fn bookmarks_need_refresh(&self, fingerprint: &str) -> Result<bool> {
        Ok(self.get_meta("bookmarks_fingerprint")?.as_deref() != Some(fingerprint))
    }

    pub fn clear_bookmarks_index(&self) -> Result<()> {
        self.conn.execute_batch("BEGIN IMMEDIATE;")?;
        let result: Result<()> = (|| {
            self.conn.execute("DELETE FROM bookmarks", [])?;
            if self.fts_enabled {
                self.conn.execute("DELETE FROM bookmarks_fts", [])?;
            }
            self.conn
                .execute("DELETE FROM meta WHERE key = 'bookmarks_fingerprint'", [])?;
            Ok(())
        })();

        match result {
            Ok(()) => {
                self.conn.execute_batch("COMMIT;")?;
                Ok(())
            }
            Err(err) => {
                let _ = self.conn.execute_batch("ROLLBACK;");
                Err(err)
            }
        }
    }

    pub fn replace_bookmarks(&self, bookmarks: &[ChromeBookmark], fingerprint: &str) -> Result<()> {
        self.conn.execute_batch("BEGIN IMMEDIATE;")?;
        let result: Result<()> = (|| {
            self.conn.execute("DELETE FROM bookmarks", [])?;
            if self.fts_enabled {
                self.conn.execute("DELETE FROM bookmarks_fts", [])?;
            }

            let mut stmt = self.conn.prepare(
                "INSERT INTO bookmarks (id, name, url, date_added, folder_path)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;

            let mut fts_stmt = if self.fts_enabled {
                Some(self.conn.prepare(
                    "INSERT INTO bookmarks_fts (bookmark_id, name, url, folder_path)
                         VALUES (?1, ?2, ?3, ?4)",
                )?)
            } else {
                None
            };

            for bookmark in bookmarks {
                stmt.execute(params![
                    bookmark.id,
                    bookmark.name,
                    bookmark.url,
                    bookmark.date_added,
                    bookmark.folder_path
                ])?;

                if let Some(ref mut fts_stmt) = fts_stmt {
                    fts_stmt.execute(params![
                        bookmark.id,
                        bookmark.name,
                        bookmark.url,
                        bookmark.folder_path
                    ])?;
                }
            }

            self.set_meta("bookmarks_fingerprint", fingerprint)?;
            Ok(())
        })();

        match result {
            Ok(()) => {
                self.conn.execute_batch("COMMIT;")?;
                Ok(())
            }
            Err(err) => {
                let _ = self.conn.execute_batch("ROLLBACK;");
                Err(err)
            }
        }
    }

    pub fn get_total_bookmarks(&self) -> Result<usize> {
        self.conn
            .query_row("SELECT COUNT(*) FROM bookmarks", [], |row| row.get(0))
    }

    pub fn load_all_bookmarks(&self) -> Result<Vec<ChromeBookmark>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, url, date_added, folder_path
             FROM bookmarks
             ORDER BY rowid",
        )?;

        let rows = stmt.query_map([], bookmark_from_row)?;
        rows.collect::<Result<Vec<_>>>()
    }

    pub fn list_bookmarks(&self, limit: usize) -> Result<Vec<ChromeBookmark>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, url, date_added, folder_path
             FROM bookmarks
             ORDER BY rowid
             LIMIT ?1",
        )?;

        let rows = stmt.query_map(params![limit as i64], bookmark_from_row)?;
        rows.collect::<Result<Vec<_>>>()
    }

    pub fn list_bookmarks_by_folder_filters(
        &self,
        folder_filters: &[String],
        limit: usize,
    ) -> Result<Vec<ChromeBookmark>> {
        let patterns: Vec<String> = folder_filters
            .iter()
            .filter_map(|raw| folder_filter_to_like_pattern(raw))
            .collect();

        if patterns.is_empty() {
            return self.list_bookmarks(limit);
        }

        let mut sql = String::from(
            "SELECT id, name, url, date_added, folder_path
             FROM bookmarks
             WHERE 1=1",
        );

        for _ in &patterns {
            sql.push_str(" AND lower(ifnull(folder_path, '')) LIKE ? ESCAPE '\\'");
        }

        sql.push_str(" ORDER BY rowid LIMIT ?");

        let mut params: Vec<&dyn ToSql> = Vec::new();
        for pattern in &patterns {
            params.push(pattern as &dyn ToSql);
        }

        let limit_param = limit as i64;
        params.push(&limit_param);

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params.as_slice(), bookmark_from_row)?;
        rows.collect::<Result<Vec<_>>>()
    }

    pub fn search_bookmarks_fts(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Option<Vec<ChromeBookmark>>> {
        if !self.fts_enabled {
            return Ok(None);
        }

        let fts_query = match build_fts_query(query) {
            Some(value) => value,
            None => return Ok(None),
        };

        let mut stmt = self.conn.prepare(
            "SELECT b.id, b.name, b.url, b.date_added, b.folder_path
             FROM bookmarks_fts
             JOIN bookmarks b ON b.id = bookmarks_fts.bookmark_id
             WHERE bookmarks_fts MATCH ?1
             ORDER BY bm25(bookmarks_fts)
             LIMIT ?2",
        )?;

        let rows = stmt.query_map(params![fts_query, limit as i64], bookmark_from_row)?;
        let results = rows.collect::<Result<Vec<_>>>()?;
        Ok(Some(results))
    }

    pub fn search_bookmarks_fts_with_folders(
        &self,
        query: &str,
        folder_filters: &[String],
        limit: usize,
    ) -> Result<Option<Vec<ChromeBookmark>>> {
        if !self.fts_enabled {
            return Ok(None);
        }

        let fts_query = match build_fts_query(query) {
            Some(value) => value,
            None => return Ok(None),
        };

        let patterns: Vec<String> = folder_filters
            .iter()
            .filter_map(|raw| folder_filter_to_like_pattern(raw))
            .collect();

        if patterns.is_empty() {
            return self.search_bookmarks_fts(query, limit);
        }

        let mut sql = String::from(
            "SELECT b.id, b.name, b.url, b.date_added, b.folder_path
             FROM bookmarks_fts
             JOIN bookmarks b ON b.id = bookmarks_fts.bookmark_id
             WHERE bookmarks_fts MATCH ?",
        );

        for _ in &patterns {
            sql.push_str(" AND lower(ifnull(b.folder_path, '')) LIKE ? ESCAPE '\\'");
        }

        sql.push_str(" ORDER BY bm25(bookmarks_fts) LIMIT ?");

        let mut values: Vec<&dyn ToSql> = Vec::new();
        values.push(&fts_query);
        for pattern in &patterns {
            values.push(pattern as &dyn ToSql);
        }
        let limit_param = limit as i64;
        values.push(&limit_param);

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(values), bookmark_from_row)?;
        let results = rows.collect::<Result<Vec<_>>>()?;

        Ok(Some(results))
    }

    fn get_meta(&self, key: &str) -> Result<Option<String>> {
        let mut stmt = self.conn.prepare("SELECT value FROM meta WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            let value: String = row.get(0)?;
            return Ok(Some(value));
        }
        Ok(None)
    }

    fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}

fn build_fts_query(query: &str) -> Option<String> {
    let mut parts = Vec::new();

    for token in query.split_whitespace() {
        let cleaned: String = token
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
            .collect();

        if cleaned.is_empty() {
            continue;
        }

        parts.push(format!("{}*", cleaned));
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

fn bookmark_from_row(row: &rusqlite::Row<'_>) -> Result<ChromeBookmark> {
    let id: String = row.get(0)?;
    let name: String = row.get(1)?;
    let url: String = row.get(2)?;
    let date_added: String = row.get(3)?;
    let folder_path: Option<String> = row.get(4)?;

    Ok(ChromeBookmark {
        id,
        name: name.clone(),
        url: url.clone(),
        date_added,
        folder_path: folder_path.clone(),
        name_lower: name.to_lowercase(),
        url_lower: url.to_lowercase(),
        folder_path_lower: folder_path.as_ref().map(|value| value.to_lowercase()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn sample_bookmark(id: &str, name: &str, url: &str, folder: Option<&str>) -> ChromeBookmark {
        ChromeBookmark {
            id: id.to_string(),
            name: name.to_string(),
            url: url.to_string(),
            date_added: "0".to_string(),
            folder_path: folder.map(ToString::to_string),
            name_lower: name.to_lowercase(),
            url_lower: url.to_lowercase(),
            folder_path_lower: folder.map(|value| value.to_lowercase()),
        }
    }

    #[test]
    fn replace_bookmarks_builds_index_and_searches() {
        let dir = tempdir().expect("tempdir");
        let db_path = dir.path().join("bookmarks.db");
        let index = BookmarkIndex::new(db_path).expect("index");

        let bookmarks = vec![
            sample_bookmark("1", "Rust Lang", "https://rust-lang.org", Some("Work/Docs")),
            sample_bookmark("2", "Example", "https://example.com", Some("Play/Read")),
        ];

        index
            .replace_bookmarks(&bookmarks, "fp-1")
            .expect("replace");
        assert!(!index.bookmarks_need_refresh("fp-1").expect("fingerprint"));
        assert_eq!(index.get_total_bookmarks().expect("count"), 2);

        let found = index
            .search_bookmarks_fts("rust", 10)
            .expect("fts")
            .expect("enabled");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, "1");
    }

    #[test]
    fn list_bookmarks_by_folder_filters_supports_hierarchy_like_matching() {
        let dir = tempdir().expect("tempdir");
        let db_path = dir.path().join("bookmarks.db");
        let index = BookmarkIndex::new(db_path).expect("index");

        let bookmarks = vec![
            sample_bookmark(
                "1",
                "Rust",
                "https://rust-lang.org",
                Some("Root/Work/Project/Rust"),
            ),
            sample_bookmark("2", "Music", "https://music.example", Some("Root/Play")),
        ];

        index
            .replace_bookmarks(&bookmarks, "fp-1")
            .expect("replace");

        let filtered = index
            .list_bookmarks_by_folder_filters(&vec!["work/project".into()], 20)
            .expect("filter");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "1");
    }

    #[test]
    fn search_bookmarks_fts_with_folders_applies_filter() {
        let dir = tempdir().expect("tempdir");
        let db_path = dir.path().join("bookmarks.db");
        let index = BookmarkIndex::new(db_path).expect("index");

        let bookmarks = vec![
            sample_bookmark(
                "1",
                "Rust Book",
                "https://doc.rust-lang.org",
                Some("Root/Work/Docs"),
            ),
            sample_bookmark(
                "2",
                "Rust Game",
                "https://game.example",
                Some("Root/Play/Games"),
            ),
        ];

        index
            .replace_bookmarks(&bookmarks, "fp-1")
            .expect("replace");

        let filtered = index
            .search_bookmarks_fts_with_folders("rust", &vec!["work".into()], 20)
            .expect("fts")
            .expect("enabled");

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "1");
    }

    #[test]
    fn clear_bookmarks_index_resets_data() {
        let dir = tempdir().expect("tempdir");
        let db_path = dir.path().join("bookmarks.db");
        let index = BookmarkIndex::new(db_path).expect("index");

        let bookmarks = vec![sample_bookmark(
            "1",
            "Rust",
            "https://rust-lang.org",
            Some("Root/Work"),
        )];
        index
            .replace_bookmarks(&bookmarks, "fp-1")
            .expect("replace");

        index.clear_bookmarks_index().expect("clear");
        assert_eq!(index.get_total_bookmarks().expect("count"), 0);
        assert!(index.bookmarks_need_refresh("fp-1").expect("refresh"));
    }
}
