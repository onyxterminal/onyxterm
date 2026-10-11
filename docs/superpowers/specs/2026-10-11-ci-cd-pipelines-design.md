---
title: CI/CD Pipelines
date: 2026-10-11
status: proposed
owner: engineering
related: docs/blueprint.md (NDW-PRD-001 §15 Workstream E, §17 QA, §18 Delivery)
---

# CI/CD Pipelines — Design

## 1. Goal

Two GitHub Actions workflows give Onyxterm reproducible releases:

- **`bump-version`** — manual-only. Reads the current version, bumps it per SemVer, commits, and tags.
- **`build`** — auto-fires on every `v*` tag push (and manual dispatch). Builds the binary on macOS, Linux, and Windows runners, packages each into a platform installer, and attaches them to the GitHub Release for the tag.

That mirrors waveterm's decoupled model: explicit human bump, automated build, automated publish.

## 2. Trigger model

| Event | Workflow | Effect |
|---|---|---|
| Manual dispatch, inputs `bump={patch,minor,major}`, `is_prerelease={true,false}` | `bump-version` | Bumps `crates/app/Cargo.toml`, commits, pushes tag `vX.Y.Z[-pr.N]` |
| `push` to tag matching `v*` | `build` | Builds for macOS arm64, Linux amd64+arm64, Windows amd64 |
| `push` to tag matching `v*` | `release` (built into `build`) | Creates/updates GitHub Release and uploads artifacts |
| Manual dispatch | `build` | Builds with the tag of the current `HEAD` (or latest `v*` if `HEAD` is a tag) |

Push-to-main builds are **out of scope** for v1 — continuous delivery to a GitHub Release without a deliberate version bump is a Phase 2 item.

## 3. Version bumping

Source of truth: `crates/app/Cargo.toml` `version` field. Other workspace crates inherit via `workspace = true`, so a single edit propagates everywhere.

`bump-version.yml` steps:

1. Checkout (using a GitHub App token or `GH_TOKEN` secret — both supported; default to `GH_TOKEN` since Onyxterm doesn't need to bypass branch protection for v1).
2. Compute next version: read current version, parse SemVer, apply the requested bump, append `-pr.<n>` if `is_prerelease` and there is no existing prerelease tag, otherwise strip the existing prerelease suffix and increment.
3. Write the new version back into `crates/app/Cargo.toml`.
4. Commit on the current branch with message `chore(release): bump version to X.Y.Z`.
5. Push the commit.
6. Create an annotated tag `vX.Y.Z` pointing at the new commit.
7. Push the tag.

Re-runs are idempotent: if the next version already exists as a tag, the workflow fails fast with a clear error instead of silently overwriting.

`cargo metadata` and `cargo build` will pick up the new version because the `app` crate re-exports it via `env!("CARGO_PKG_VERSION")` into `Info.plist` and bundle metadata — verified by the existing `scripts/bundle-macos.sh` which reads the same field.

## 4. Build matrix

| Runner | Target | Produces |
|---|---|---|
| `macos-latest` | `aarch64-apple-darwin` | `Onyxterm.app` → `Onyxterm-X.Y.Z-macos-arm64.zip` |
| `ubuntu-latest` | `x86_64-unknown-linux-gnu` | `target/release/onyxterm` → `onyxterm_X.Y.Z_amd64.deb` and `Onyxterm-X.Y.Z-x86_64.AppImage` |
| `ubuntu-24.04-arm` | `aarch64-unknown-linux-gnu` | Same as above, `arm64` / `aarch64` |
| `windows-latest` | `x86_64-pc-windows-msvc` | `target/release/onyxterm.exe` → `Onyxterm-X.Y.Z-x86_64.msi` |

Intel macOS is intentionally deferred: the blueprint (NDW-PRD-001 §1) commits to Apple Silicon as the primary macOS target. Adding `macos-13` to the matrix is a one-line change later.

## 5. Per-platform packaging

### 5.1 macOS — `.app` + zip

Reuse the existing `scripts/bundle-macos.sh` end-to-end. It already:

- Reads the version from `crates/app/Cargo.toml`.
- Runs `cargo build --release -p app`.
- Stages `Contents/MacOS/onyxterm`, `Contents/Info.plist`, `Contents/Resources/Onyxterm.icns`.
- Ad-hoc code-signs and verifies.

CI adds:

- Regenerate `assets/icons/Onyxterm.icns` via `scripts/make-icon.sh` (idempotent, ~30 s).
- Zip the `.app` directory (preserves macOS bundle structure; the user double-clicks the `.app` inside).

No notarization for v1 (signing skipped). Gatekeeper warning on first open is acceptable.

### 5.2 Linux — `.deb` + `.AppImage`

Use `cargo-deb` for `.deb`. Add as a workspace dev-dependency and a `[package.metadata.deb]` block on `crates/app/Cargo.toml`:

```toml
[package.metadata.deb]
maintainer = "Onyxterm <dev@onyxterm.local>"
copyright = "2026, Onyxterm"
license-file = ["../../LICENSE", "0"]
depends = "$auto"
section = "utils"
priority = "optional"
assets = [
    ["target/release/onyxterm", "usr/bin/", "755"],
    ["packaging/linux/onyxterm.desktop", "usr/share/applications/", "644"],
    ["packaging/linux/onyxterm.png", "usr/share/icons/hicolor/256x256/apps/", "644"],
]
```

`cargo deb --target x86_64-unknown-linux-gnu --no-build` produces the `.deb`.

For `.AppImage`, the path of least resistance:

1. Stage an AppDir at `target/appimage/Onyxterm.AppDir/` containing:
   - `AppRun` — bash script that execs `./usr/bin/onyxterm "$@"`.
   - `onyxterm.desktop` — XDG entry pointing at `AppRun`.
   - `usr/bin/onyxterm` — the release binary.
   - `usr/share/icons/hicolor/256x256/apps/onyxterm.png` — generated from `assets/logo.png`.
2. Download `appimagetool-x86_64.AppImage` and `appimagetool-aarch64.AppImage` from AppImageKit's GitHub release, extract, and run against the AppDir.

No FPM, no Ruby, no Zig — keeps the matrix green on stock Ubuntu runners.

### 5.3 Windows — `.msi`

Use `cargo-wix` (cargo subcommand wrapping WiX 3.x). Add as a workspace dev-dependency and a `[package.metadata.wix]` block on `crates/app/Cargo.toml`:

```toml
[package.metadata.wix]
installer-name = "Onyxterm-X.Y.Z-x86_64.msi"
[package.metadata.wix.product]
name = "Onyxterm"
manufacturer = "Onyxterm"
[package.metadata.wix.msi]
banner = "packaging/windows/banner.bmp"
dialog = "packaging/windows/dialog.bmp"
```

CI installs WiX 3.x via Chocolatey (`choco install wixtoolset`) and runs `cargo wix --no-build`. The MSI embeds the `.exe`, places a Start Menu shortcut, and an uninstall entry — no extra scripting.

### 5.4 Icon generation per platform

The existing `scripts/make-icon.sh` handles macOS (`sips` + `iconutil`). For Linux/Windows we need equivalent steps that run on Ubuntu and Windows runners:

- Linux runner: `magick assets/logo.png -resize 256x256 packaging/linux/onyxterm.png` (ImageMagick is pre-installed on `ubuntu-latest`).
- Windows runner: `magick assets/logo.png -define icon:auto-resize=256,128,96,64,48,32,16 packaging/windows/onyxterm.ico` (ImageMagick is pre-installed on `windows-latest`).

These are inline in the workflow YAML — small enough not to warrant a separate script.

## 6. Publishing

A single job (`release`) inside `build.yml`, dependent on all four build jobs:

1. Uses `softprops/action-gh-release` with `fail_on_unmatched_files: true`.
2. Tag: `${{ github.ref_name }}` for tag-triggered runs; the latest `v*` for manual dispatch.
3. Uploads: the four platform artifacts (`.zip`, two `.deb`, two `.AppImage`, `.msi`) with stable filenames.
4. Generates release notes from commits since the previous tag (`generate_release_notes: true`).

No external publishing to Snapcraft, WinGet, Homebrew, S3 — out of scope per the chosen distribution answer.

## 7. Files to add

```
.github/
  workflows/
    bump-version.yml
    build.yml
packaging/
  linux/
    onyxterm.desktop
    onyxterm.png                # generated in CI, not committed
  windows/
    banner.bmp                  # optional, deferred if WiX defaults are fine
    dialog.bmp                  # optional
crates/app/
  Cargo.toml                    # add [package.metadata.deb] + [package.metadata.wix] sections
Cargo.toml                      # add cargo-deb + cargo-wix to [workspace.dev-dependencies] (or [workspace.metadata])
```

That's it. No changes to `crates/`, no changes to `scripts/`, no changes to `docs/blueprint.md`.

## 8. Workflow file contents (commit-worthy)

Both workflows use `actions/checkout@v6`, `dtolnay/rust-toolchain@stable` (matching `rust-toolchain.toml`), and `Swatinem/rust-cache@v2` keyed on `Cargo.lock`. `cargo build` runs once per matrix leg; packaging steps run after.

## 9. Out of scope (explicit non-goals)

- **Signing/notarization** — ad-hoc only for v1.
- **Push-to-main dev builds** — no continuous artifacts.
- **Snapcraft, Flatpak, WinGet, Homebrew** — store distribution deferred.
- **Intel macOS** — single macos-latest (arm64) runner.
- **Universal macOS binary** — single arch per platform zip.
- **Cross-compilation from one runner** — matrix uses native runners per arch.
- **Automatic changelog curation** — GitHub's autogenerated notes only.
- **Linux ARM .deb testing** — produced but not install-tested in CI.

## 10. Acceptance

Done when, after `bump-version.yml` is run with `bump=patch,is_prerelease=false`:

- A commit `chore(release): bump version to X.Y.Z` appears on `main`.
- A tag `vX.Y.Z` exists locally and on origin.
- `build.yml` produces a GitHub Release `vX.Y.Z` containing exactly six files:
  - `Onyxterm-X.Y.Z-macos-arm64.zip`
  - `Onyxterm-X.Y.Z-x86_64.AppImage`
  - `Onyxterm-X.Y.Z-aarch64.AppImage`
  - `onyxterm_X.Y.Z_amd64.deb`
  - `onyxterm_X.Y.Z_arm64.deb`
  - `Onyxterm-X.Y.Z-x86_64.msi`
- Each artifact extracts/installs to a working binary that prints the same `X.Y.Z` from `env!("CARGO_PKG_VERSION")`.
- A `workflow_dispatch` run on `build.yml` against any existing `v*` tag re-produces the same set without overwriting the Release.

## 11. Risks

| Risk | Likelihood | Mitigation |
|---|---|---|
| `cargo-deb` needs glibc 2.35 on Ubuntu 22.04 runner | Med | Pin to `ubuntu-24.04-arm` and `ubuntu-latest` (already 24.04 since Aug 2025); document fallback to older target. |
| `cargo-wix` adds non-trivial compile time on Windows | Med | Cache WiX output by `Cargo.lock` hash. |
| Apple Silicon runner picks up `x86_64` sysroot inadvertently | Low | Set `CARGO_BUILD_TARGET` explicitly in the macOS job. |
| WiX install prompts for license acceptance | Med | Pass `--accept-license` to choco or vendor WiX as a build artifact. |
| GitHub Release API rate limit on rapid back-to-back tags | Low | `softprops/action-gh-release` already retries with backoff. |