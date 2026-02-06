# Repository Guidelines

## Project Structure & Module Organization
- `src/main.rs`: CLI entrypoint, Alfred JSON output, command routing.
- `src/cli.rs`: `structopt` command definitions (`search`, `refresh`, `stats`).
- `src/bookmark.rs`: browser bookmark discovery (Chromium-family paths) and JSON parsing/cache.
- `src/index_db.rs`: SQLite index + FTS5 queries, refresh fingerprint logic.
- `src/searcher.rs`: ranking, fuzzy matching, folder-filter parsing/matching helpers.
- `res/chrome-workflow/`: minimal Alfred workflow template (`info.plist`, `run.sh`, icon).
- `scripts/`: packaging/bootstrap/dev helpers.

## Build, Test, and Development Commands
- `cargo test`: run all unit tests (`#[cfg(test)]` in Rust modules).
- `cargo build --release`: optimized binary at `target/release/alfred-chrome-bookmarks`.
- `./scripts/build_workflow.sh --version 0.1.0`: package `.alfredworkflow` into `dist/`.
- `./scripts/bootstrap_workflow.sh -- --version 0.1.0`: test + build + package in one step.
- `./create_alfred_workflow.sh ...`: compatibility wrapper for `build_workflow.sh`.

## Coding Style & Naming Conventions
- Language: Rust 2021.
- Formatting: always run `cargo fmt` before committing.
- Indentation: Rust default (4 spaces), no tabs.
- Naming: `snake_case` for functions/variables/modules, `CamelCase` for types.
- Keep hot paths allocation-light; prefer database-side filtering for default search.

## Testing Guidelines
- Add tests in the same module file using `#[cfg(test)]`.
- Test names should describe behavior, e.g. `search_bookmarks_fts_with_folders_applies_filter`.
- New search/index logic must include success and edge-case tests (empty query, folder filters, escaping).
- Run `cargo test` locally before PR or merge.

## Commit & Pull Request Guidelines
- Current history is minimal (`init`); use clear imperative commit messages.
- Recommended pattern: `feat: ...`, `fix: ...`, `perf: ...`, `chore: ...`.
- Keep each PR focused; include:
  - what changed,
  - why it changed,
  - test evidence (`cargo test`, packaging command output).
- For workflow-facing behavior changes, update `README.md` and `ALFRED_WORKFLOW_GUIDE.md` in the same PR.

## Security & Configuration Tips
- Never commit personal bookmark files or local Alfred data.
- Use `ALFRED_CHROME_BOOKMARKS_PATH` only for local override/testing.
- Validate `BINARY_PATH` in Alfred variables when debugging launch issues.
