#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
app_name="Onyx"
executable_name="Onyx"
cargo_package="app"
cargo_binary="app"
bundle="$repo_root/target/release/$app_name.app"
install_target="$HOME/Applications/$app_name.app"

should_install=false
for argument in "$@"; do
  case "$argument" in
    --install) should_install=true ;;
    *) echo "usage: $0 [--install]" >&2; exit 64 ;;
  esac
done

version="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$repo_root/crates/$cargo_package/Cargo.toml" | head -n 1)"
if [ -z "$version" ]; then
  echo "cannot read version from crates/$cargo_package/Cargo.toml" >&2
  exit 65
fi

cargo build --release -p "$cargo_package" --manifest-path "$repo_root/Cargo.toml"

rm -rf "$bundle"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
cp "$repo_root/target/release/$cargo_binary" "$bundle/Contents/MacOS/$executable_name"
cp "$repo_root/packaging/Info.plist" "$bundle/Contents/Info.plist"
cp "$repo_root/assets/icons/$app_name.icns" "$bundle/Contents/Resources/$app_name.icns"
printf 'APPL????' > "$bundle/Contents/PkgInfo"

plutil -replace CFBundleShortVersionString -string "$version" "$bundle/Contents/Info.plist"
plutil -replace CFBundleVersion -string "$version" "$bundle/Contents/Info.plist"
plutil -lint "$bundle/Contents/Info.plist"

if [ -n "${APPLE_SIGNING_IDENTITY:-}" ]; then
  sign_args=(--force --options runtime --timestamp "--sign" "$APPLE_SIGNING_IDENTITY")
  if [ -f "$repo_root/packaging/Entitlements.plist" ]; then
    sign_args=(--force --options runtime --timestamp "--entitlements" "$repo_root/packaging/Entitlements.plist" "--sign" "$APPLE_SIGNING_IDENTITY")
  fi
else
  sign_args=(--force --options runtime "--sign" "-")
  echo "warning: ad-hoc signing — Gatekeeper will warn" >&2
fi

codesign "${sign_args[@]}" "$bundle/Contents/MacOS/$executable_name"
codesign "${sign_args[@]}" "$bundle"
codesign --verify --verbose "$bundle"
echo "built $bundle ($version)"

if [ "$should_install" = false ]; then
  exit 0
fi

mkdir -p "$HOME/Applications"
rm -rf "$install_target"
cp -R "$bundle" "$install_target"
echo "installed $install_target"
