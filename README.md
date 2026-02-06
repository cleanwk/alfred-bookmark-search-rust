# Alfred Chromium Bookmarks

一个极简、极速的 Alfred Workflow：专注于 Chromium 系浏览器书签搜索，支持目录过滤，不再包含 tag 体系。

## 特性

- 极致快：SQLite FTS5 + 本地索引，默认搜索路径尽量走数据库查询。
- 多浏览器支持：Chrome、Arc、Dia、Brave、Edge、Vivaldi、Chromium、Opera 等。
- 目录过滤：支持多级目录匹配（如 `work/project`），并支持内联语法。
- Alfred 友好：`cb` 普通搜索，`cbf` 模糊搜索。

## 从源码到可用 Workflow

```bash
git clone <your-repo-url>
cd alfred-pinboard-rs
./scripts/bootstrap_workflow.sh -- --version 0.1.0
open dist/AlfredChromeBookmarks.alfredworkflow
```

安装后在 Alfred 输入：

- `cb rust`
- `cb folder:work/project rust`

## 搜索语法

### 1. 普通搜索

```bash
alfred-chrome-bookmarks search rust async
```

### 2. 目录过滤参数

```bash
alfred-chrome-bookmarks search --folders "work/project" rust
```

### 3. 内联目录过滤（推荐）

支持前缀：`folder:` `dir:` `path:` `in:`

```bash
alfred-chrome-bookmarks search "rust folder:work/project"
alfred-chrome-bookmarks search "in:research/ai transformer"
```

## 命令

```bash
alfred-chrome-bookmarks search [--folders ...] [--fuzzy] [--limit N] <query...>
alfred-chrome-bookmarks refresh
alfred-chrome-bookmarks stats
```

## 速度优化点

- 默认 `search`：优先 FTS5 查询（避免全量扫描）。
- 目录过滤：在 SQL 侧先做 `LIKE` 过滤，再返回结果。
- 模糊搜索：仅在 `cbf` 或 `--fuzzy` 时启用（更慢但容错更高）。
- 书签索引按 fingerprint 增量刷新，避免重复解析。
- SQLite 使用 `WAL` + `NORMAL` + `mmap` 配置。

## 环境变量

- `ALFRED_CHROME_BOOKMARKS_PATH`: 强制指定书签文件路径。
- `alfred_workflow_data`: Alfred 数据目录（自动使用）。
- `alfred_workflow_cache`: Alfred 缓存目录（自动使用）。

示例：

```bash
export ALFRED_CHROME_BOOKMARKS_PATH="$HOME/Library/Application Support/Arc/Default/Bookmarks"
```

## 打包脚本

```bash
./scripts/build_workflow.sh --version 0.1.0
```

兼容入口：

```bash
./create_alfred_workflow.sh --version 0.1.0
```

## 测试

```bash
cargo test
```

## License

MIT
