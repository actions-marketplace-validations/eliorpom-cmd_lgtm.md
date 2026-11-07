#!/usr/bin/env bash
# Assembles the launch page into one folder: the page itself, the preview GIFs and the font.
# Used by the Pages workflow and for local checks. Usage: site/build.sh <out-dir>
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
out="${1:?usage: site/build.sh <out-dir>}"
mkdir -p "$out/widgets" "$out/fonts"
cp "$root"/site/index.html "$root"/site/style.css "$root"/site/app.js "$root"/site/favicon.svg "$root"/site/CNAME "$out/"
cp "$root"/examples/preview/*.gif "$out/widgets/"
cp "$root"/assets/fonts/MonaSans-Regular.ttf "$root"/assets/fonts/MonaSans-Medium.ttf "$out/fonts/"
echo "Site assembled in $out"
