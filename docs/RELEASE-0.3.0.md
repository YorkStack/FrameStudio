# Version 0.3.0 — validation

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
| Native release / CI | Recorded below after packaging | Pending final package checks |

Changes: one direct optional native dependency, tauri-plugin-dialog 2.8.4, provides the OS folder selector. Four lockfile packages were added; the SBOM contains 684 locked components, including development and platform-specific dependencies. No new database table or migration: settings.language is an additive validated JSON field. Existing profiles default to German. Minimal Tauri capability grants and CSP remain unchanged; fixed key-help destinations are a local-only core operation.

Known distribution limits: macOS Apple Silicon is the initial download target. Ad-hoc signing does not provide Apple Developer ID identity or notarization. A newly built binary may need renewed keychain app authorization; session-only keys are available without plaintext persistence. Windows CI is prepared; a prepared workflow is not a Windows test result. Third-party audit exceptions and unmaintained transitive packages are listed in DEPENDENCIES.md.

Tests use synthetic assets, not personal projects. Initial development test failures were corrected and rerun: inaccessible fieldset legends, a case-sensitive label mismatch, and an unintended Vite reload while the separate release tree was written. Build/artifact directories are now excluded from dev-server watching.
