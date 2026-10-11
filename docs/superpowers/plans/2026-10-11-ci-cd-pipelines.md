# CI/CD Pipelines Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add two GitHub Actions workflows (`bump-version` + `build`) that produce six platform installers per release tag and attach them to a GitHub Release.

**Architecture:** Tag-push trigger mirrors waveterm's decoupled model. `bump-version.yml` is manual-dispatch-only and edits `crates/app/Cargo.toml`. `build.yml` fires on `v*` tag push (or manual dispatch), runs a 4-leg matrix (macos arm64, linux amd64, linux arm64, windows amd64), packages per platform with native tooling (existing `bundle-macos.sh`, `cargo-deb`, `appimagetool`, `cargo-wix`), then a dependent `release` job uploads everything to the GitHub Release via `softprops/action-gh-release`.

**Tech Stack:** GitHub Actions, Rust stable (matches `rust-toolchain.toml`), cargo-deb, cargo-wix, AppImageKit appimagetool, ImageMagick (pre-installed on Ubuntu/Windows runners), softprops/action-gh-release, Swatinem/rust-cache, dtolnay/rust-toolchain.

**Spec:** `docs/superpowers/specs/2026-10-11-ci-cd-pipelines-design.md`

## Global Constraints

- Rust stable toolchain — `rust-toolchain.toml` pins channel `stable`, components `["rustfmt", "clippy"]`.
- Single source of truth for app version: `crates/app/Cargo.toml` `version` field. Workspace crates inherit via `workspace = true`.
- Apple Silicon focus (NDW-PRD-001 §1). Intel macOS deferred — single macos-latest (arm64) leg.
- Linux: `ubuntu-latest` (amd64) + `ubuntu-24.04-arm` (arm64). Requires glibc ≥ 2.35.
- Windows: `windows-latest` amd64 only.
- No code signing / notarization in v1. macOS bundle uses ad-hoc signature via existing `scripts/bundle-macos.sh`.
- Six artifacts per release (named exactly):
  - `Onyxterm-X.Y.Z-macos-arm64.zip`
  - `Onyxterm-X.Y.Z-x86_64.AppImage`
  - `Onyxterm-X.Y.Z-aarch64.AppImage`
  - `onyxterm_X.Y.Z_amd64.deb`
  - `onyxterm_X.Y.Z_arm64.deb`
  - `Onyxterm-X.Y.Z-x86_64.msi`
- `scripts/bundle-macos.sh` and `scripts/make-icon.sh` must continue to work locally unchanged.
- No comments in committed code (per `CLAUDE.md`).

## Review Focus

The spec implies these behaviors that no task's mechanical test pins; the task that owns the code must verify each one explicitly:

1. **Tag collision fails fast** — re-running `bump-version` with the same inputs must NOT silently overwrite an existing `v*N_TAG`. → Task 3.
2. **Version string round-trips through every artifact** — every produced binary must print `X.Y.Z` from `env!("CARGO_PKG_VERSION")`. → Task 5.
3. **Manual dispatch on `build.yml` against existing tag does not recreate the release** — `softprops/action-gh-release` must overwrite, not duplicate. → Task 4.
4. **WiX license prompt does not hang CI on Windows** — `choco install wixtoolset` must pass `--accept-license` or vendor the binaries. → Task 4.
5. **AppImage `AppRun` resolves `$@` even when called via desktop launcher** — the wrapper must pass arguments through, not just `exec` without them. → Task 2.

## File Structure

```
.github/workflows/bump-version.yml              [NEW] manual SemVer bump + tag
.github/workflows/build.yml                     [NEW] matrix build + per-platform packaging + release
packaging/linux/onyxterm.desktop                 [NEW] XDG entry
packaging/linux/AppRun                          [NEW] AppImage launcher script
Cargo.toml                                      [MODIFY] workspace dev-dependencies
crates/app/Cargo.toml                           [MODIFY] [package.metadata.deb] + [package.metadata.wix]
```

`packaging/linux/onyxterm.png` and `packaging/windows/onyxterm.ico` are generated inside CI from `assets/logo.png` — not committed.

---

### Task 1: Cargo packaging metadata and dev-deps

**Files:**
- Modify: `Cargo.toml:31-32` (workspace dev-dependencies, after the existing `[profile.release]` block — append before EOF)
- Modify: `crates/app/Cargo.toml` (append metadata blocks after the existing `[dependencies]` block)

**Interfaces:**
- Consumes: nothing
- Produces: `cargo metadata` succeeds with the new fields visible; `cargo deb` and `cargo wix` (after `cargo install`) can read the metadata

- [ ] **Step 1: Install cargo-deb and cargo-wix dev tools locally to verify version compatibility**

```bash
Run: `cargo install --list | grep -E '^(cargo-deb|cargo-wix)'`
Expected: exit 0 (printed) OR exit 1 (not installed — fine, we'll install next step).
```

If not installed:
```bash
cargo install cargo-deb --locked
cargo install cargo-wix --locked
```

Verify both install successfully. If either fails on `rust-version = "1.85"`, record the failure and escalate — do NOT relax the toolchain pin in `rust-toolchain.toml`.

- [ ] **Step 2: Add dev-deps to workspace `Cargo.toml`**

Append before EOF:
```toml
[workspace.metadata]
# cargo-deb (Linux .deb) and cargo-wix (Windows .msi) read per-package blocks under [package.metadata.*].
# These workspace-level entries exist purely to anchor the package metadata in one place
# when packages reuse identical config. Per-package overrides live in crates/app/Cargo.toml.
```

If those entries are not needed by the tool defaults, skip this block. Test:

```bash
Run: `cargo metadata --format-version 1 --no-deps | python3 -c "import sys, json; d=json.load(sys.stdin); print('OK')"`
Expected: prints `OK`.
```

- [ ] **Step 3: Add `[package.metadata.deb]` block to `crates/app/Cargo.toml`**

After the existing `[dependencies]` block, append:

```toml
[package.metadata.deb]
maintainer = "Onyxterm <dev@onyxterm.local>"
copyright = "2026, Onyxterm"
license-file = []
section = "utils"
priority = "optional"
depends = "$auto"
extended-description = "Onyxterm — local-first Rust terminal workspace."
description = "Onyxterm terminal workspace"
assets = [
    ["target/release/onyxterm", "usr/bin/", "755"],
    ["packaging/linux/onyxterm.desktop", "usr/share/applications/", "644"],
    ["packaging/linux/onyxterm.png", "usr/share/icons/hicolor/256x256/apps/", "644"],
]
```

- [ ] **Step 4: Add `[package.metadata.wix]` block to `crates/app/Cargo.toml`**

Append after the `[package.metadata.deb]` block:

```toml
[package.metadata.wix]
installer-name = "Onyxterm-{version}-x86_64.msi"

[package.metadata.wix.product]
name = "Onyxterm"
manufacturer = "Onyxterm"
version = "{version}"

[package.metadata.wix.msi]
install-step-ui = "basic"
```

- [ ] **Step 5: Verify metadata is readable**

```bash
Run: `cargo metadata --format-version 1 | python3 -c "import sys, json; d=json.load(sys.stdin); pkg=[p for p in d['packages'] if p['name']=='app'][0]; assert 'deb' in pkg['metadata']; assert 'wix' in pkg['metadata']; print('OK')"`
Expected: prints `OK`.
```

If either key is missing, re-check the block placement — it must be inside the `[package]` table of `crates/app/Cargo.toml`, not the workspace root.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/app/Cargo.toml
git commit -m "feat(packaging): add cargo-deb and cargo-wix metadata"
```

---

### Task 2: Linux packaging assets (desktop entry + AppRun)

**Files:**
- Create: `packaging/linux/onyxterm.desktop`
- Create: `packaging/linux/AppRun`
- Create: `packaging/linux/.gitkeep` (so the directory tracks even before PNG is generated)

**Interfaces:**
- Consumes: nothing
- Produces: an XDG `.desktop` file that `desktop-file-validate` accepts; an executable `AppRun` script that exits 0 when invoked with `--help` against a no-op binary

- [ ] **Step 1: Create the directory**

```bash
mkdir -p packaging/linux
```

- [ ] **Step 2: Write `packaging/linux/onyxterm.desktop`**

```ini
[Desktop Entry]
Type=Application
Name=Onyxterm
GenericName=Terminal Workspace
Comment=Local-first Rust terminal workspace and AI agent control center
Exec=onyxterm %U
Icon=onyxterm
Terminal=false
Categories=System;TerminalEmulator;Development;
StartupNotify=true
StartupWMClass=Onyxterm
```

- [ ] **Step 3: Validate the desktop entry**

```bash
Run: `command -v desktop-file-validate >/dev/null && desktop-file-validate packaging/linux/onyxterm.desktop || echo "desktop-file-validate not installed, skipping (CI does not install it either; the .desktop file is verified by structural review only)"`
Expected: either exit 0 OR the explicit "skipping" message. Any other output is a failure.
```

If `desktop-file-validate` flags a warning, fix the field and re-run.

- [ ] **Step 4: Write `packaging/linux/AppRun`**

```bash
#!/usr/bin/env bash
set -euo pipefail
SELF="$(readlink -f "$0")"
HERE="$(dirname "$SELF")"
exec "$HERE/usr/bin/onyxterm" "$@"
```

The `$@` forwarding satisfies Review Focus #5 — desktop-launched invocations receive file paths via `%U` and must reach the binary.

- [ ] **Step 5: Make `AppRun` executable and verify it parses**

```bash
chmod +x packaging/linux/AppRun
Run: `bash -n packaging/linux/AppRun && echo "syntax OK"`
Expected: prints `syntax OK`.
```

- [ ] **Step 6: Add `.gitkeep` and commit**

```bash
touch packaging/linux/.gitkeep
git add packaging/linux/
git commit -m "feat(packaging): add Linux desktop entry and AppRun"
```

---

### Task 3: bump-version.yml workflow

**Files:**
- Create: `.github/workflows/bump-version.yml`

**Interfaces:**
- Consumes: `crates/app/Cargo.toml` (reads + writes `version`), git history on the current branch
- Produces: one commit on the current branch + one tag `vX.Y.Z[-rc.N]` pushed to origin

- [ ] **Step 1: Create the directory**

```bash
mkdir -p .github/workflows
```

- [ ] **Step 2: Write `.github/workflows/bump-version.yml`**

```yaml
name: Bump Version
run-name: "branch: ${{ github.ref_name }}; bump: ${{ inputs.bump }}; prerelease: ${{ inputs.is_prerelease }}"

on:
  workflow_dispatch:
    inputs:
      bump:
        description: SemVer bump
        type: choice
        required: true
        default: patch
        options:
          - patch
          - minor
          - major
      is_prerelease:
        description: Mark as prerelease
        type: boolean
        required: true
        default: false

permissions:
  contents: write

jobs:
  bump:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v6
        with:
          token: ${{ secrets.GITHUB_TOKEN }}
          fetch-depth: 0

      - name: Configure git
        run: |
          git config user.name "github-actions[bot]"
          git config user.email "41898282+github-actions[bot]@users.noreply.github.com"

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable

      - name: Compute next version
        id: next-version
        env:
          BUMP: ${{ inputs.bump }}
          IS_PRERELEASE: ${{ inputs.is_prerelease }}
        run: |
          set -euo pipefail
          CURRENT=$(sed -n 's/^version = "\(.*\)"$/\1/p' crates/app/Cargo.toml | head -n1)
          if [ -z "$CURRENT" ]; then
            echo "::error::cannot read version from crates/app/Cargo.toml" >&2
            exit 1
          fi

          BASE=$(echo "$CURRENT" | sed -E 's/-.*$//')
          MAJOR=$(echo "$BASE" | cut -d. -f1)
          MINOR=$(echo "$BASE" | cut -d. -f2)
          PATCH=$(echo "$BASE" | cut -d. -f3)

          case "$BUMP" in
            major) MAJOR=$((MAJOR + 1)); MINOR=0; PATCH=0 ;;
            minor) MINOR=$((MINOR + 1)); PATCH=0 ;;
            patch) PATCH=$((PATCH + 1)) ;;
            *) echo "::error::unknown bump: $BUMP" >&2; exit 1 ;;
          esac

          if [ "$IS_PRERELEASE" = "true" ]; then
            NEXT="$MAJOR.$MINOR.$PATCH-rc.1"
          else
            NEXT="$MAJOR.$MINOR.$PATCH"
          fi

          if git ls-remote --tags origin "v$NEXT" | grep -q "v$NEXT"; then
            echo "::error::tag v$NEXT already exists on origin" >&2
            exit 1
          fi

          {
            echo "NEXT=$NEXT"
            echo "CURRENT=$CURRENT"
          } >> "$GITHUB_OUTPUT"

      - name: Update version
        run: |
          sed -i "s/^version = \".*\"/version = \"${{ steps.next-version.outputs.NEXT }}\"/" crates/app/Cargo.toml

      - name: Verify version reads back
        run: |
          ACTUAL=$(sed -n 's/^version = "\(.*\)"$/\1/p' crates/app/Cargo.toml | head -n1)
          if [ "$ACTUAL" != "${{ steps.next-version.outputs.NEXT }}" ]; then
            echo "::error::version mismatch: expected ${{ steps.next-version.outputs.NEXT }}, got $ACTUAL" >&2
            exit 1
          fi

      - name: Commit and tag
        run: |
          git add crates/app/Cargo.toml
          git commit -m "chore(release): bump version to ${{ steps.next-version.outputs.NEXT }}"
          git tag -a "v${{ steps.next-version.outputs.NEXT }}" -m "Release v${{ steps.next-version.outputs.NEXT }}"
          git push origin HEAD
          git push origin "v${{ steps.next-version.outputs.NEXT }}"
```

- [ ] **Step 3: Static validation**

```bash
Run: `command -v actionlint >/dev/null 2>&1 && actionlint .github/workflows/bump-version.yml || python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/bump-version.yml')); print('YAML OK')"`
Expected: prints `YAML OK` (or `actionlint` exits 0).
```

- [ ] **Step 4: Manual dry-run on a feature branch (Review Focus #1)**

```bash
Run: `bash -n .github/workflows/bump-version.yml && echo "shell snippets parse"`
Expected: prints `shell snippets parse`.
```

The full collision check (Review Focus #1) requires GitHub — verify by:
1. Create a branch `test/bump-dry`.
2. Push it.
3. Run the workflow via Actions tab with `bump=patch,is_prerelease=false`.
4. Re-run the same inputs.
5. Expected: second run fails with `tag v* already exists on origin` in the step log.

If GitHub is not reachable from your test environment, verify the logic by running the `Compute next version` step locally against the current version:

```bash
Run: `CURRENT=$(sed -n 's/^version = "\(.*\)"$/\1/p' crates/app/Cargo.toml | head -n1); BUMP=patch; NEXT=$(echo "$CURRENT" | sed -E 's/-.*$//' | awk -F. -v b="$BUMP" '{if(b=="major"){print $1+1".0.0"} else if(b=="minor"){print $1"."$2+1".0"} else {print $1"."$2"."$3+1}}'); echo "would create: v$NEXT"`
Expected: prints `would create: v<next-patch>`.
```

- [ ] **Step 5: Commit**

```bash
git add .github/workflows/bump-version.yml
git commit -m "feat(ci): add bump-version workflow"
```

---

### Task 4: build.yml workflow (matrix + per-platform packaging + release)

**Files:**
- Create: `.github/workflows/build.yml`

**Interfaces:**
- Consumes: tag push (`v*`) or manual dispatch
- Produces: 6 release artifacts uploaded to the GitHub Release for the tag

- [ ] **Step 1: Write `.github/workflows/build.yml`**

Each matrix leg runs on its native host target, so no `--target` flag is needed in cargo invocations — `cargo build`, `cargo deb`, and `cargo wix` all produce host-target binaries. The `matrix.target` field is reserved for future cross-compilation but unused in v1.

```yaml
name: Build
run-name: "build ${{ github.ref_name }}"

on:
  push:
    tags:
      - "v*"
  workflow_dispatch:

permissions:
  contents: write

env:
  BIN_NAME: onyxterm

jobs:
  build:
    name: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        include:
          - os: macos-latest
            platform: macos
            artifact_id: app-macos-arm64
          - os: ubuntu-latest
            platform: linux
            arch: x86_64
            deb_arch: amd64
            artifact_id: app-linux-x86_64
          - os: ubuntu-24.04-arm
            platform: linux
            arch: aarch64
            deb_arch: arm64
            artifact_id: app-linux-aarch64
          - os: windows-latest
            platform: windows
            artifact_id: app-windows-x86_64
    steps:
      - name: Resolve version
        id: version
        run: |
          VERSION=$(sed -n 's/^version = "\(.*\)"$/\1/p' crates/app/Cargo.toml | head -n1)
          echo "VERSION=$VERSION" >> "$GITHUB_OUTPUT"

      - uses: actions/checkout@v6

      - uses: dtolnay/rust-toolchain@stable

      - uses: Swatinem/rust-cache@v2
        with:
          cache-on-failure: true

      - name: Build
        run: cargo build --release --bin app -p app

      - name: Stage binary as ${{ env.BIN_NAME }}
        if: matrix.platform == 'linux'
        run: mv "target/release/app" "target/release/${{ env.BIN_NAME }}"
        shell: bash

      # === Linux icon ===
      - name: Generate Linux icon
        if: matrix.platform == 'linux'
        run: |
          mkdir -p packaging/linux
          magick assets/logo.png -resize 256x256 packaging/linux/onyxterm.png
        shell: bash

      # === macOS packaging ===
      - name: Regenerate .icns
        if: matrix.platform == 'macos'
        run: scripts/make-icon.sh

      - name: Bundle .app
        if: matrix.platform == 'macos'
        run: scripts/bundle-macos.sh

      - name: Zip .app
        if: matrix.platform == 'macos'
        run: |
          cd target/release
          zip -r "Onyxterm-${{ steps.version.outputs.VERSION }}-macos-arm64.zip" Onyxterm.app
        shell: bash

      - name: Upload macOS artifact
        if: matrix.platform == 'macos'
        uses: actions/upload-artifact@v4
        with:
          name: ${{ matrix.artifact_id }}
          path: target/release/Onyxterm-${{ steps.version.outputs.VERSION }}-macos-arm64.zip

      # === Linux .deb ===
      - name: Build .deb
        if: matrix.platform == 'linux'
        run: cargo deb --no-build
        shell: bash

      - name: Stage AppImage AppDir
        if: matrix.platform == 'linux'
        run: |
          APPDIR=stage/Onyxterm.AppDir
          mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/icons/hicolor/256x256/apps"
          cp "target/release/${{ env.BIN_NAME }}" "$APPDIR/usr/bin/onyxterm"
          cp packaging/linux/onyxterm.desktop "$APPDIR/onyxterm.desktop"
          cp packaging/linux/onyxterm.png "$APPDIR/usr/share/icons/hicolor/256x256/apps/onyxterm.png"
          cp packaging/linux/AppRun "$APPDIR/AppRun"
          chmod +x "$APPDIR/AppRun" "$APPDIR/usr/bin/onyxterm"
        shell: bash

      - name: Download appimagetool
        if: matrix.platform == 'linux'
        run: |
          curl -fsSL -o appimagetool.AppImage \
            "https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-${{ matrix.arch }}.AppImage"
          chmod +x appimagetool.AppImage
          ./appimagetool.AppImage --appimage-extract
          ls squashfs-root/AppRun
        shell: bash

      - name: Build AppImage
        if: matrix.platform == 'linux'
        run: |
          ARCH="${{ matrix.arch }}" ./squashfs-root/AppRun stage/Onyxterm.AppDir \
            "Onyxterm-${{ steps.version.outputs.VERSION }}-${{ matrix.arch }}.AppImage"
        shell: bash

      - name: Rename .deb for release naming
        if: matrix.platform == 'linux'
        run: |
          DEB=$(ls target/debian/*.deb | head -n1)
          NEW="target/debian/onyxterm_${{ steps.version.outputs.VERSION }}_${{ matrix.deb_arch }}.deb"
          mv "$DEB" "$NEW"
        shell: bash

      - name: Upload Linux artifacts
        if: matrix.platform == 'linux'
        uses: actions/upload-artifact@v4
        with:
          name: ${{ matrix.artifact_id }}
          path: |
            target/debian/*.deb
            Onyxterm-*.AppImage

      # === Windows .msi ===
      - name: Generate Windows .ico
        if: matrix.platform == 'windows'
        shell: pwsh
        run: |
          New-Item -ItemType Directory -Force -Path packaging/windows | Out-Null
          magick assets/logo.png -define icon:auto-resize=256,128,96,64,48,32,16 packaging/windows/onyxterm.ico

      - name: Install WiX
        if: matrix.platform == 'windows'
        shell: pwsh
        run: choco install wixtoolset -y --accept-license --no-progress

      - name: Build MSI
        if: matrix.platform == 'windows'
        run: cargo wix --no-build

      - name: Upload Windows artifact
        if: matrix.platform == 'windows'
        uses: actions/upload-artifact@v4
        with:
          name: ${{ matrix.artifact_id }}
          path: target/wix/*.msi

  release:
    needs: build
    if: github.event_name == 'push' && startsWith(github.ref, 'refs/tags/v')
    runs-on: ubuntu-latest
    steps:
      - name: Download all artifacts
        uses: actions/download-artifact@v4
        with:
          path: artifacts

      - name: Publish GitHub Release
        uses: softprops/action-gh-release@v2
        with:
          tag_name: ${{ github.ref_name }}
          generate_release_notes: true
          fail_on_unmatched_files: true
          files: |
            artifacts/app-macos-arm64/*.zip
            artifacts/app-linux-x86_64/*.deb
            artifacts/app-linux-x86_64/*.AppImage
            artifacts/app-linux-aarch64/*.deb
            artifacts/app-linux-aarch64/*.AppImage
            artifacts/app-windows-x86_64/*.msi
```

- [ ] **Step 2: Static validation**

```bash
Run: `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/build.yml')); print('YAML OK')"`
Expected: prints `YAML OK`.
```

If `actionlint` is available locally:

```bash
Run: `command -v actionlint >/dev/null 2>&1 && actionlint .github/workflows/build.yml || true`
Expected: exit 0 (no output) OR the explicit `true` fallback message.
```

- [ ] **Step 3: Cross-check artifact filenames against the spec**

```bash
Run: `python3 -c "
import re
with open('.github/workflows/build.yml') as f:
    yaml_text = f.read()
required = [
    'Onyxterm-{ver}-macos-arm64.zip',
    'Onyxterm-{ver}-x86_64.AppImage',
    'Onyxterm-{ver}-aarch64.AppImage',
    'onyxterm_{ver}_amd64.deb',
    'onyxterm_{ver}_arm64.deb',
    'Onyxterm-{ver}-x86_64.msi',
]
yaml_text_norm = yaml_text.replace('\${{ steps.version.outputs.VERSION }}', '{ver}')
missing = [r for r in required if r.replace('{ver}', '') not in yaml_text_norm]
print('MISSING:', missing) if missing else print('OK')"
Expected: prints `OK`.
```

If `MISSING:` is printed, the corresponding section of `build.yml` is malformed — fix and re-run.

- [ ] **Step 4: Verify WiX non-interactive install (Review Focus #4)**

The plan assumes `choco install wixtoolset -y --accept-license --no-progress` does not prompt. To verify locally on a Windows runner or via Chocolatey docs:

```bash
Run on Windows runner: `choco install wixtoolset -y --accept-license --no-progress --what-if`
Expected: prints the install plan and exits 0 without prompting.
```

If `--accept-license` is insufficient, escalate: vendor `wixtoolset` binaries from `https://github.com/wixtoolset/wix3/releases` and add them to `$PATH` via `mv` in the step instead of using choco.

- [ ] **Step 5: Commit**

```bash
git add .github/workflows/build.yml
git commit -m "feat(ci): add build workflow with per-platform packaging"
```

---

### Task 5: End-to-end verification

**Files:** none — verification only

**Interfaces:**
- Consumes: a real tag push (or workflow_dispatch on `build.yml` against an existing tag)
- Produces: a confirmed GitHub Release with the 6 named artifacts and matching version strings

- [ ] **Step 1: Push a test tag**

```bash
git tag -a v0.0.0-test -m "CI smoke test"
git push origin v0.0.0-test
```

If the repo does not allow tag pushes from your local environment, use the GitHub UI or `gh release create v0.0.0-test --generate-notes --draft` to create the tag, then push it.

- [ ] **Step 2: Watch the workflow**

```bash
Run: `gh run watch --exit-status`
Expected: workflow run completes with all 4 matrix legs green + release job green.
```

- [ ] **Step 3: Confirm the release (Review Focus #2 + #3)**

```bash
Run: `gh release view v0.0.0-test --json assets --jq '.assets[] | "\(.name) \(.size) bytes"'`
Expected: prints 6 asset lines:
  - `Onyxterm-0.0.0-test-macos-arm64.zip`
  - `Onyxterm-0.0.0-test-x86_64.AppImage`
  - `Onyxterm-0.0.0-test-aarch64.AppImage`
  - `onyxterm_0.0.0-test_amd64.deb`
  - `onyxterm_0.0.0-test_arm64.deb`
  - `Onyxterm-0.0.0-test-x86_64.msi`
```

Re-trigger via Actions → `build` → `Run workflow` with the existing `v0.0.0-test` branch selected:

```bash
Run: `gh release view v0.0.0-test --json assets --jq '.assets | length'`
Expected: prints `6` (not 12) — release was overwritten, not duplicated (Review Focus #3).
```

- [ ] **Step 4: Verify version strings round-trip (Review Focus #2)**

For each binary:

```bash
# macOS zip — extract and inspect bundled Info.plist
unzip -p artifacts/app-macos-arm64/Onyxterm-0.0.0-test-macos-arm64.zip Onyxterm.app/Contents/Info.plist | plutil -extract CFBundleShortVersionString raw -
Expected: 0.0.0-test

# Linux binary — same binary ships in .deb and .AppImage
ar x artifacts/app-linux-x86_64/onyxterm_0.0.0-test_amd64.deb data.tar.zst
unzstd -c data.tar.zst | tar -xO ./usr/bin/onyxterm > /tmp/onyxterm-test
chmod +x /tmp/onyxterm-test
strings /tmp/onyxterm-test | grep -m1 '^0\.0\.0-test$'
Expected: prints `0.0.0-test`.

# Windows .msi — extract via msiexec /a or 7z
# Note: cargo-wix packages the cargo binary name (app.exe), not Onyxterm.exe.
# This is internal — the Start Menu shortcut displays "Onyxterm" and points at app.exe.
7z x artifacts/app-windows-x86_64/Onyxterm-0.0.0-test-x86_64.msi -o/tmp/msi-extract
strings /tmp/msi-extract/ProgramFilesFolder/Onyxterm/app.exe 2>/dev/null | grep -m1 '^0\.0\.0-test$' || \
  python3 -c "import re; data=open('/tmp/msi-extract/ProgramFilesFolder/Onyxterm/app.exe','rb').read(); print(re.search(rb'0\\.0\\.0-test', data).decode())"
Expected: prints `0.0.0-test`.
```

If any binary lacks the version string, the bundle metadata block (Task 1) is missing the version interpolation — most likely the WiX `version = "{version}"` placeholder did not expand. Inspect the artifact's manifest in `target/wix/` on the Windows runner.

- [ ] **Step 5: Clean up the test tag**

```bash
gh release delete v0.0.0-test --yes
git tag -d v0.0.0-test
git push origin --delete v0.0.0-test
```

If the repo does not have releases enabled or the user prefers to keep the smoke-test release for inspection, leave it and document its presence in the PR description.

- [ ] **Step 6: Update the README**

Append to `README.md` after the existing "Build the app" section:

```markdown
## Releases

Bump the version and cut a tag:

1. GitHub → Actions → `Bump Version` → Run workflow → choose `patch`/`minor`/`major` and whether the tag is a prerelease.
2. The workflow commits the bump, creates `vX.Y.Z`, and `Build` runs automatically.
3. The GitHub Release for the tag carries the macOS zip, Linux `.deb` and `.AppImage` (amd64 + arm64), and Windows `.msi`.
```

- [ ] **Step 7: Commit the README**

```bash
git add README.md
git commit -m "docs(ci): document release workflow in README"
```