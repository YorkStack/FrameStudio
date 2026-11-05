# Version 0.3.0 — validation

*This validation report is available in English only.*

Validated on 2026-10-31, macOS 27.2, Apple M2 Pro, 16 GiB RAM.

| Area | Implemented / executed | Result |
|---|---|---|
| Beginner flow | Sidebar single-image entry, persistent collection, same budgeted core | GUI creates exactly one local test image |
| Language | 528 central UI keys, additional literal/diagnostic catalogs, persisted DE/EN choice | Catalog coverage and GUI language/reload/return tests passed; user-authored content preserved |
| Access grants | Active/revoked filters, compact rows, details, individual revocation | Creation, one-time copy, recipient editing and independent revocation tested |
| Workspace | Native folder chooser; writable empty or existing workspace; next-start switch | Empty/existing directory, occupied folder preservation, traversal and symlink tests passed |
| API key help | Fixed official Gemini/OpenAI/Anthropic key creation destinations | Links reviewed 2026-10-31; no key read or provider call for help |
| Core | cargo test --locked | 133 passed, 5 explicitly ignored (interactive keychain/visual proof tests) |
| GUI | npm run test:e2e, real Rust core with isolated synthetic stores | 35 passed, including 1,000-asset pagination, providers, approvals, exports and minimum-window dialog actions |
| Code / build | cargo clippy --locked --all-targets -- -D warnings; frontend production build | Passed |
| npm dependencies | npm audit --audit-level=low | 0 known vulnerabilities at check time |
| Public source scan | Allowlisted new repository, credential patterns/private identifiers/runtime files/PNG text metadata checked | No findings; personal workspace and old Git history excluded |
| Live provider requests | Not used for this release validation | No development spending; no fresh provider success claimed |
| Native macOS app | Release app on an isolated workspace: single-image entry, real folder picker, DE/EN menus and restart | Passed; English persists after restart; deterministic core image created |
| Download artifacts | App signature verification, DMG checksum verification and read-only mount, ZIP integrity, per-file hash comparison | Passed; DMG and ZIP contain the same app; binary/source privacy scans have no findings |
| Windows x64 | Native Windows Server 2025 build, NSIS installation, synthetic credential store, REST/MCP, restart, exports, GUI startup and working-area bounds | Passed: 128 core/integration tests plus 1 explicit native credential test; [Windows report](WINDOWS.md) |

Changes: one direct optional native dependency, tauri-plugin-dialog 2.8.4, provides the OS folder selector. Four lockfile packages were added; the SBOM contains 684 locked components, including development and platform-specific dependencies. No new database table or migration: settings.language is an additive validated JSON field. Existing profiles default to German. Minimal Tauri capability grants and CSP remain unchanged; fixed key-help destinations are a local-only core operation.

Known distribution limits: Download targets are macOS Apple Silicon and Windows Intel/AMD x64. Ad-hoc signing does not provide Apple Developer ID identity or notarization. A newly built binary may need renewed keychain app authorization; session-only keys are available without plaintext persistence. The Windows installer is unsigned. Complete manual Windows 10/11 interaction and live-provider acceptance remain open. Third-party audit exceptions and unmaintained transitive packages are listed in DEPENDENCIES.md.

Tests use synthetic assets, not personal projects. Initial development test failures were corrected and rerun: inaccessible fieldset legends, a case-sensitive label mismatch, and an unintended Vite reload while the separate release tree was written. Build/artifact directories are now excluded from dev-server watching.

Performance sample: the Chromium GUI with the actual Rust CLI and 1,000 synthetic assets rendered 72 gallery cards in 876 ms; the next page took 365 ms. Mean frame interval was 16.67 ms, maximum 16.8 ms; Chromium JS heap was 28.7 MiB. These measurements include the browser test bridge and are not native app startup or total-process memory measurements.

Release artifacts were built from the public source checkout with Rust home-path remapping. The subsequent Windows-only SBOM script correction does not change the macOS app contents. The source and binary scans check credential-shaped values, private project identifiers, personal home paths, runtime databases and image text metadata; they do not constitute a general security certification.

The Windows installer is built from commit `7871eb654ba2ee6aafe74a2fb94ca518f7846b1b`; the existing macOS tag and binaries are unchanged. Windows fixes add platform-specific packaging, native menu compatibility, portable acceptance scripts and small-screen/window sizing corrections. No Windows runtime dependency, permission or schema change was added. Two targeted local Playwright tests for small-window controls passed.
