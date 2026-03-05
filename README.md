# Browser Bookmarks

Ultra-fast Alfred workflow for searching local browser bookmarks. Built with Rust and SQLite FTS5.

## Usage

Search your browser bookmarks via the `bms` keyword.

* <kbd>↩︎</kbd> Open bookmark in default browser
* <kbd>⌘</kbd><kbd>↩︎</kbd> Copy URL to clipboard
* <kbd>⌥</kbd><kbd>↩︎</kbd> Preview full URL
* <kbd>⌘</kbd><kbd>Y</kbd> Quick Look bookmark URL

### Folder Filtering

Filter by bookmark folder using `#folder` or `folder:path` syntax.

```
bms #work rust
bms folder:work/project rust
bms in:research/ai transformer
```

Multiple folder filters can be combined:

```
bms #work #docs rust
```

### Fuzzy Search

Search with typo tolerance via the `bmf` keyword.

```
bmf rsut
```

### Actions

Manage the workflow via the `bma` keyword.

* **Refresh Index** — Rescan bookmarks and rebuild search index
* **Show Stats** — Display total bookmark count
* **Open Workflow Guide** — View local setup guide
* **Open README** — View this file

### Hotkey

Default: <kbd>⌃</kbd><kbd>⌥</kbd><kbd>⌘</kbd><kbd>B</kbd> — opens the `bms` search (configurable in Alfred).

## Configuration

Configure options in the [Workflow's Configuration](https://www.alfredapp.com/help/workflows/user-configuration/):

| Variable | Default | Description |
|----------|---------|-------------|
| `ALFRED_CHROME_BOOKMARKS_BROWSER` | *(auto-detect)* | Restrict to a specific browser |
| `RESULT_LIMIT` | `36` | Max results for exact search (`bms`) |
| `FUZZY_LIMIT` | `24` | Max results for fuzzy search (`bmf`) |
| `BINARY_PATH` | *(auto)* | Override path to the binary |

### Supported Browsers

**Chromium-based:** Chrome, Brave, Edge, Arc, Dia, Vivaldi, Opera, Sidekick, Chromium

**Firefox-based:** Firefox, Zen

Browser value examples: `chrome`, `brave`, `edge`, `arc`, `dia`, `firefox`, `zen`, `opera`, `vivaldi`, `sidekick`

## Building from Source

```bash
git clone https://github.com/cleanwk/alfred-bookmark-search-rust.git
cd alfred-bookmark-search-rust
./scripts/build_workflow.sh --version 0.2.0
open dist/AlfredChromeBookmarks.alfredworkflow
```

## Performance

- **FTS5 full-text search** with BM25 relevance ranking
- **Incremental indexing** — only re-indexes when bookmarks change (fingerprint-based)
- **2-second TTL** on index checks to avoid redundant work during rapid typing
- **SQLite WAL mode** with memory-mapped I/O for maximum read throughput
- **Fuzzy pre-selection** — FTS5 narrows candidates before fuzzy ranking

## Testing

```bash
cargo test
```

## License

MIT
