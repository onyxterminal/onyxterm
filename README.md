# Onyxterm

Onyxterm — a local-first, Rust-native terminal workspace and AI agent control center (macOS-first).

```
cargo build
cargo test
cargo run -p app
```

## Build the app

`scripts/bundle-macos.sh` builds an ad-hoc signed `target/release/Onyxterm.app`.
Pass `--install` to also copy it to `~/Applications/Onyxterm.app`.

```
scripts/bundle-macos.sh --install
open ~/Applications/Onyxterm.app
```

Once installed, Onyxterm launches from Finder, Spotlight, or the Dock. To regenerate
`assets/icons/Onyxterm.icns` from `assets/logo.png`, run `scripts/make-icon.sh`.

## Releases

Bump the version and cut a tag:

1. GitHub → Actions → `Bump Version` → Run workflow → choose `patch`/`minor`/`major` and whether the tag is a prerelease.
2. The workflow commits the bump, creates `vX.Y.Z`, and `Build` runs automatically.
3. The GitHub Release for the tag carries the macOS zip, Linux `.deb` and `.AppImage` (amd64 + arm64), and Windows `.msi`.
