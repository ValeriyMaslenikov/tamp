#!/usr/bin/env bash
# Downloads static ffmpeg/ffprobe builds and places them where Tauri expects
# sidecar binaries (src-tauri/binaries/<name>-<target-triple>).
#
# Builds are GPL-licensed static binaries from https://ffmpeg.martin-riedl.de.
# Run once after cloning, and again to update the bundled FFmpeg version.
set -euo pipefail

ARCH="${1:-$(uname -m)}"
case "$ARCH" in
  arm64|aarch64) RIEDL_ARCH="arm64" TRIPLE="aarch64-apple-darwin" ;;
  x86_64|amd64)  RIEDL_ARCH="amd64" TRIPLE="x86_64-apple-darwin" ;;
  *) echo "Unsupported arch: $ARCH" >&2; exit 1 ;;
esac

DIR="$(cd "$(dirname "$0")/.." && pwd)/src-tauri/binaries"
mkdir -p "$DIR"

for BIN in ffmpeg ffprobe; do
  DEST="$DIR/$BIN-$TRIPLE"
  if [[ -x "$DEST" ]]; then
    echo "✓ $DEST already present, skipping (delete it to re-fetch)"
    continue
  fi
  TMP="$(mktemp -d)"
  trap 'rm -rf "$TMP"' EXIT
  echo "↓ Downloading $BIN (macos/$RIEDL_ARCH)…"
  curl -fsSL -o "$TMP/$BIN.zip" \
    "https://ffmpeg.martin-riedl.de/redirect/latest/macos/$RIEDL_ARCH/release/$BIN.zip"
  unzip -oq "$TMP/$BIN.zip" -d "$TMP/out"
  cp "$TMP/out/$BIN" "$DEST"
  chmod +x "$DEST"
  xattr -d com.apple.quarantine "$DEST" 2>/dev/null || true
  codesign -fs - "$DEST"
  rm -rf "$TMP"
  echo "✓ $DEST"
done

"$DIR/ffmpeg-$TRIPLE" -version | head -1
"$DIR/ffprobe-$TRIPLE" -version | head -1
