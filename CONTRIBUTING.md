# Contributing to FrameStudio

*Deutsch: Beiträge und Issues auf Deutsch sind willkommen.*

## Toolchain

Pinned versions: Node 24.22.1 (`.nvmrc`), npm 12.2.1 and Rust 1.98.1 (`rust-toolchain.toml`). Python 3.11+ is used only to build the SBOM.

- macOS: Xcode Command Line Tools
- Windows: Visual Studio C++ build tools, Windows SDK and WebView2 (see [docs/WINDOWS.md](docs/WINDOWS.md))

## Build and test

```sh
npm ci
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --locked --no-default-features
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets -- -D warnings
```

Browser UI tests against the real Rust core (macOS):

```sh
cargo build --manifest-path src-tauri/Cargo.toml --locked
cargo build --manifest-path src-tauri/Cargo.toml --locked --example session_ui_fixture
npx playwright install chromium
npm run test:e2e
```

Desktop app:

```sh
npm run tauri -- build                    # macOS: src-tauri/target/release/bundle/
npm run tauri -- build --bundles nsis     # Windows installer
```

For public builds, remove local paths from compiler diagnostics:

```sh
RUSTFLAGS="--remap-path-prefix=$HOME=/build" npm run tauri -- build --ci
```

Lockfiles pin all dependencies; binary hashes may still differ between build environments and signing identities.

## Documentation screenshots

All screenshots in `docs/images/` and `docs/de/images/` are produced by one script from a synthetic workspace with the free test provider and the bundled style previews. No real keys, projects or paid requests are involved.

```sh
cargo build --manifest-path src-tauri/Cargo.toml --locked
npx playwright test -c scripts/docs-screenshots.config.ts
```

Run it again after UI changes and commit the updated images together with the change. `docs/images/windows-start.png` is a native Windows capture from CI and is not produced by this script.

## Conventions

- **Interface texts** exist in German and English: German in `src/i18n.ts`, English in `src/en.json` (plus `en-literals.json` and `en-errors.json`). `npm run build` runs `scripts/check-i18n.mjs` and fails on missing keys.
- **Documentation:** English in `docs/` is the primary version; keep the German version in `docs/de/` in sync (same file names). User-facing changes go into the Quickstart/User Guide in both languages.
- **Secrets:** never commit API keys, access tokens, runtime databases (`*.sqlite3`), personal images or local paths. Tests use synthetic data only. GitHub secret scanning with push protection is enabled.
- **CI:** do not add `[skip ci]` to commits that change code. Every push to `main` and every pull request runs "Desktop checks" on macOS and Windows.
- Record notable changes in [CHANGELOG.md](CHANGELOG.md).

## Releases

1. Update the version in `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`, and move the "Unreleased" entries in `CHANGELOG.md` under the new version.
2. Make sure "Desktop checks" is green on the release commit.
3. Tag that exact commit (`vX.Y.Z`) and build all release artifacts from it, so the tag reproduces every binary attached to the release.
4. Attach DMG, Windows installer, SBOMs and checksums; add a validation report like [docs/RELEASE-0.3.0.md](docs/RELEASE-0.3.0.md).
