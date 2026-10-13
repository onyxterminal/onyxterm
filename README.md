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

## Appearance settings

Open **Onyxterm → Settings** or press **⌘,**. Choose an installed terminal font, adjust
terminal text and UI size, and toggle dimming for unfocused panes. JetBrains Mono is
bundled and selected by default. Its font license is in
`assets/fonts/jetbrains-mono/OFL.txt`.

Choose from Apple Light, Onyx Dark, Nord, Dracula, Solarized Dark, and Solarized Light.
Select **Customize** to copy the current theme, then edit its appearance and terminal
colors using `#RRGGBB` values. Settings and the custom theme are saved per profile.

## Releases

Bump the version and cut a tag:

1. GitHub → Actions → `Bump Version` → Run workflow → choose `patch`/`minor`/`major` and whether the tag is a prerelease.
2. The workflow commits the bump, creates `vX.Y.Z`, and `Build` runs automatically.
3. The GitHub Release for the tag carries the macOS zip, Linux `.deb` and `.AppImage` (amd64 + arm64), and Windows `.msi`.

## Code signing & notarization

Until a Developer ID Application certificate is available, the bundled
`.app` is ad-hoc signed and the DMG ships unsigned. Gatekeeper will warn
on first launch; right-click → Open once to clear the quarantine.

The build workflow already has the Developer ID + notarization steps
staged and inert. To enable them:

1. Export the Developer ID Application certificate as `.p12`, then base64
   encode it: `base64 -i cert.p12 | pbcopy`.
2. Seed the notarytool keychain profile on a one-off macOS runner:
   `xcrun notarytool store-credentials --team-id <TEAMID> <PROFILE>`.
3. Add these repository secrets in GitHub → Settings → Secrets:
   - `APPLE_SIGNING_IDENTITY` — e.g. `Developer ID Application: Onyx (TEAMID)`
   - `APPLE_TEAM_ID` — 10-character Apple team identifier
   - `APPLE_CERT_P12_BASE64` — the base64 blob from step 1
   - `APPLE_CERT_PASSWORD` — the `.p12` password
   - `APPLE_KEYCHAIN_PROFILE` — the notarytool profile name from step 2
4. Create `packaging/Entitlements.plist` with the entitlements the app
   needs (sandbox, JIT, etc.). The staged codesign step names the file
   and will fail without it.
5. Remove the `## ENABLED:` comment header on the signing block in
   `.github/workflows/build.yml`. The block runs only when
   `APPLE_SIGNING_IDENTITY` is non-empty, so no other workflow change
   is needed.
