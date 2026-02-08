#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")" && pwd)"
WORKFLOW_DIR="$ROOT_DIR/res/chrome-workflow"
OUTPUT="$ROOT_DIR/dist/AlfredChromeBookmarks.alfredworkflow"

echo "==> Building release binary..."
cargo build --release --manifest-path "$ROOT_DIR/Cargo.toml"

echo "==> Copying binary to workflow directory..."
cp "$ROOT_DIR/target/release/alfred-chrome-bookmarks" "$WORKFLOW_DIR/alfred-chrome-bookmarks"
chmod +x "$WORKFLOW_DIR/alfred-chrome-bookmarks"

echo "==> Packaging .alfredworkflow..."
mkdir -p "$ROOT_DIR/dist"
rm -f "$OUTPUT"
(cd "$WORKFLOW_DIR" && zip -qr "$OUTPUT" .)

echo "==> Done!"
echo "   $OUTPUT"
echo ""
echo "Double-click the .alfredworkflow file to install in Alfred."
