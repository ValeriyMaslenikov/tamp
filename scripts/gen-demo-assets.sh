#!/usr/bin/env bash
# Regenerates the synthetic demo assets used for README screenshots
# (public/mock/ is gitignored and must never contain real recordings).
set -euo pipefail
cd "$(dirname "$0")/.."
FF=src-tauri/binaries/ffmpeg-$(uname -m | sed 's/arm64/aarch64/')-apple-darwin
mkdir -p public/mock
for i in 1 2 3 4 5 6; do
  "$FF" -y -v error -f lavfi -i "testsrc2=size=640x400:rate=1,hue=h=$((i*55)):s=2" \
    -frames:v 1 -vf scale=160:-2 -q:v 5 "public/mock/thumb$i.jpg"
done
"$FF" -y -v error -f lavfi -i "testsrc2=size=960x600:rate=30,hue=h=200:s=2" -t 3 \
  -c:v libx264 -preset fast -crf 28 -an -movflags +faststart public/mock/preview.mp4
echo "done — remember to delete public/mock before building the app"
