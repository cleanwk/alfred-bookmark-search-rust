#!/usr/bin/env bash
set -euo pipefail

WORKFLOW_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

emit_binary_not_found() {
  cat <<JSON
{"items":[{"title":"Binary not found","subtitle":"Set BINARY_PATH in workflow variables or place alfred-chrome-bookmarks in the workflow folder","icon":{"path":"icons/error.png"},"valid":false}]}
JSON
}

resolve_binary_path() {
  local bin="${BINARY_PATH:-}"
  if [[ -n "$bin" && -x "$bin" ]]; then
    printf '%s\n' "$bin"
    return 0
  fi

  local candidates=(
    "$WORKFLOW_DIR/alfred-chrome-bookmarks"
    "$HOME/.local/bin/alfred-chrome-bookmarks"
    "$HOME/.cargo/bin/alfred-chrome-bookmarks"
    "/usr/local/bin/alfred-chrome-bookmarks"
    "/opt/homebrew/bin/alfred-chrome-bookmarks"
  )

  local candidate
  for candidate in "${candidates[@]}"; do
    if [[ -x "$candidate" ]]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done

  return 1
}

run_binary() {
  local bin
  if ! bin="$(resolve_binary_path)"; then
    emit_binary_not_found
    return 1
  fi
  "$bin" "$@"
}

notify_user() {
  local message="${1:-Done}"
  local safe_message="${message//\"/\\\"}"
  osascript -e "display notification \"$safe_message\" with title \"Chromium Bookmarks\"" >/dev/null 2>&1 || true
}

extract_subtitle_from_json() {
  local payload="${1:-}"
  printf '%s' "$payload" | sed -n 's/.*"subtitle":"\([^"]*\)".*/\1/p' | head -n 1
}

dispatch_action() {
  local arg="${1:-}"
  case "$arg" in
    open:*)
      open "${arg#open:}"
      ;;
    copy:*)
      printf '%s' "${arg#copy:}" | pbcopy
      notify_user "URL copied"
      ;;
    action:refresh)
      local refresh_output
      if refresh_output="$(run_binary refresh 2>/dev/null)"; then
        local refresh_msg
        refresh_msg="$(extract_subtitle_from_json "$refresh_output")"
        notify_user "${refresh_msg:-Bookmarks index refreshed}"
      else
        notify_user "Refresh failed"
        return 1
      fi
      ;;
    action:stats)
      local stats_output
      if stats_output="$(run_binary stats 2>/dev/null)"; then
        local stats_msg
        stats_msg="$(extract_subtitle_from_json "$stats_output")"
        notify_user "${stats_msg:-Stats loaded}"
      else
        notify_user "Stats failed"
        return 1
      fi
      ;;
    action:open_readme)
      open "$WORKFLOW_DIR/README.md"
      ;;
    action:open_guide)
      open "$WORKFLOW_DIR/ALFRED_WORKFLOW_GUIDE.md"
      ;;
    "")
      exit 0
      ;;
    *)
      exit 1
      ;;
  esac
}

case "${1:-}" in
  dispatch)
    shift || true
    dispatch_action "${1:-}"
    ;;
  *)
    if ! bin="$(resolve_binary_path)"; then
      emit_binary_not_found
      exit 1
    fi
    exec "$bin" "$@"
    ;;
esac
