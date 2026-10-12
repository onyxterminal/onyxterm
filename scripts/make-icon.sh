#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
glyph="$repo_root/assets/logo.png"
output="$repo_root/assets/icons/Onyx.icns"
work_dir="$repo_root/target/icon"
master="$work_dir/master.png"
iconset="$work_dir/Onyx.iconset"

rm -rf "$work_dir"
mkdir -p "$iconset" "$(dirname "$output")"

swift "$repo_root/scripts/compose-icon.swift" "$glyph" "$master"

for size in 16 32 128 256 512; do
  retina=$((size * 2))
  sips -z "$size" "$size" "$master" --out "$iconset/icon_${size}x${size}.png" >/dev/null
  sips -z "$retina" "$retina" "$master" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done

iconutil -c icns "$iconset" -o "$output"
echo "wrote $output"
