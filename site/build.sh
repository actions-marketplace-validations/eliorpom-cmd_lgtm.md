#!/usr/bin/env bash
# Assembles the launch page into one folder: the page itself, the preview GIFs, the social image
# and the screenshots of real profiles. Used by the Pages workflow and for local checks.
# Usage: site/build.sh <out-dir>
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
out="${1:?usage: site/build.sh <out-dir>}"
mkdir -p "$out/widgets" "$out/wild"
cp "$root"/site/index.html "$root"/site/style.css "$root"/site/app.js "$root"/site/favicon.svg "$root"/site/CNAME "$root"/site/wild.json "$out/"
if [ -f "$root/site/og.png" ]; then cp "$root/site/og.png" "$out/"; fi
cp "$root"/site/wild/* "$out/wild/"
cp "$root"/examples/preview/*.gif "$out/widgets/"
echo "Site assembled in $out"
