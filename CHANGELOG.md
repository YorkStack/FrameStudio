# Changelog

All notable changes to FrameStudio. Versions follow [Semantic Versioning](https://semver.org/).

## Unreleased

### Windows
- Windows x64 NSIS installer (per-user, English/German, installs WebView2 if missing). Built from commit `7871eb6`, after the `v0.3.0` tag; see [docs/WINDOWS.md](docs/WINDOWS.md).
- Native Windows menu actions and About dialog metadata, Ctrl+N labeling.
- Settings and dialog actions stay reachable on small displays; the initial window fits the monitor working area.
- Windows SBOM console encoding fixed.

### Documentation
- English is now the primary documentation language throughout; a complete German version is in `docs/de/`.
- New screenshots for projects, prompt sections, styles, characters, series approvals, budgets and access tokens, in English and German.
- `SECURITY.md` is now a vulnerability reporting policy; the design notes moved to `docs/SECURITY-DESIGN.md`.
- Added `CONTRIBUTING.md`, issue and pull request templates and Dependabot configuration.
- Removed unused iOS/Android icon sets.

## 0.3.0 — 2026-10-31

- Beginner flow: **Create single image** entry in the sidebar with a persistent "Single images" project.
- German/English interface with persisted language choice.
- Access grants: active/revoked filters, compact rows, individual revocation.
- Workspace switching with the native folder chooser (takes effect at next start; old workspace is kept).
- Help links to the official key pages of Gemini, OpenAI and Anthropic.
- First public release: macOS Apple Silicon DMG (ad-hoc signed), CycloneDX SBOM, English Quickstart and User Guide.

See the [release validation](docs/RELEASE-0.3.0.md).

## 0.2.0

- Three-section prompt editor (scene, character, individual instruction) with per-project templates.
- Claude prompt assistant can improve one section or all three at once.
- Project trash with restore, character display portraits, style preview library with bundled neutral previews.
- SBOM generated at build time and exportable from the settings.

## Earlier versions

0.1.x was developed privately before the public repository was created; its history is not included here.
