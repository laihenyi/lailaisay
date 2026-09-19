# lailaisay release checklist

The maintained branch is `main`. The repository has a new Rust-only root history under the name `lailaisay`; predecessor source, obsolete model scanning and historical development documents are not part of this release tree.

## Implementation

- Rust workspace crates, UI titles, tray menus, executable names, macOS bundle name, Windows setup and CI artifacts use lailaisay.
- Whisper inference uses whisper.cpp through Rust bindings. Native libraries and platform packaging scripts are required; no predecessor-language compiler or application runtime is required.
- Existing storage paths and installation identities remain stable for upgrades. See [compatibility notes](rust/README.md#existing-user-data).
- Windows prefers a detected GPU and retries on an isolated CPU worker if the GPU path fails.
- The installer verifies upgrades from the previous Rust product name, shortcut cleanup, data preservation and installed inference in CI.

## Before publishing

- [ ] Confirm the current main CI run is green, including the renamed Windows installer smoke tests.
- [ ] Install the Windows setup on the user's target machine after exiting the old app; confirm Chinese text, numbers, paste, GPU status and retained settings/models.
- [ ] Build and test the final macOS Apple Silicon bundle, including permissions, hotkeys, paste and normal window focus.
- [ ] Choose distribution channel and release version. Store acceptance has not been established by these tests.
- [ ] Configure and verify Windows signing. The Windows setup is unsigned.
- [ ] macOS outside-App-Store distribution: package (`rust/scripts/package-macos-app.sh`), then Developer ID + notarize (`rust/scripts/notarize-macos.sh` with `CODESIGN_IDENTITY` / App Store Connect API key env or `NOTARYTOOL_PROFILE`). Verify `spctl --assess --type execute -vv dist/lailaisay.app` and `xcrun stapler validate dist/lailaisay.app`. Do not commit `.p8` keys or issuer secrets. Local packages without that step remain ad-hoc signed. See [rust/MAC_SMOKE.md](rust/MAC_SMOKE.md#outside-app-store-developer-id--notarization).
- [ ] Review privacy disclosures, third-party licenses and release notes for the selected channel.
- [ ] Publish durable release assets with checksums. CI artifacts expire after 14 days.

Build guides: [Windows](rust/WINDOWS.md), [macOS](rust/MAC_SMOKE.md).
