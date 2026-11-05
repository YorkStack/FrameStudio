# Reproducible build and dependencies

*Deutsch: [de/DEPENDENCIES.md](de/DEPENDENCIES.md)*

Checked on 2026-10-31. npm packages via `npm view <name> version`, Rust via the crates.io API (latest non-yanked, non-prerelease version), then lockfiles. No automatic updates. Tauri 3 alpha is deliberately excluded.

| Component | Version | Purpose |
|---|---|---|
| Tauri Rust / Build | 2.12.5 / 2.7.4 | Native webview, bundles, no embedded Chromium |
| Tauri JS API / CLI | 2.12.4 | Typed IPC and build tooling |
| React / TypeScript / Vite | 19.3.1 / 7.0.2 / 8.3.5 | Local interface and reproducible frontend build |
| Rust / Node / npm | 1.98.1 / 24.22.1 / 12.2.1 | Pinned developer tools, Node LTS; not needed to run the installed app |
| rusqlite | 0.41.1 bundled | Transactional embedded SQLite |
| rmcp | 3.8.0 | Official MCP SDK and real stdio client test |
| reqwest / tokio / axum | 0.13.5 / 1.54.1 / 0.8.9 | TLS providers, asynchronous jobs and loopback API |
| scraper | 0.29.0 | Inert HTML DOM parser for unambiguous vendor price tables; never runs remote JavaScript |
| keyring | 4.2.0 | Native OS credential stores; no file fallback |
| core-foundation / security-framework-sys (macOS only) | 0.10.1 / 2.17.0 | Direct metadata query without reading the password; on Apple, keyring's standard attribute access reads the secret internally |
| image / zip / lopdf | 0.25.10 / 9.2.0 / 0.46.0 | Real portable image encoders, ZIP and PDF without a helper runtime |
| ttf-parser | 0.25.1 | Metrics of the bundled OFL font only, for Unicode PDF |
| lucide-react | 1.66.0 | Consistent accessible UI icons, no remote resources |
| Playwright | 1.64.1 | Browser acceptance against the real Rust core |
| tauri-plugin-dialog | 2.8.4 | Native folder chooser (since 0.3.0) |

The two macOS metadata dependencies were already present transitively in `Cargo.lock`; their exact versions were kept. The Security framework constants used and their behavior were checked on 2026-10-31 against the installed official macOS SDK (`Security.framework/Headers/SecItem.h`).

Further small Rust helpers: serde/serde_json for data contracts, uuid for collision-free IDs, chrono for timestamps, base64 for image transport, fs2 for the process lock, sha2/subtle for hashes and token comparison, directories for platform-standard locations, zeroize for overwriting session keys in memory; tempfile for tests only. Exact transitive versions are in `Cargo.lock` / `package-lock.json`.

`zeroize = 1.9.1` is referenced directly to overwrite stored session keys and provider read copies when they are released. The version was already present transitively. [Official API documentation](https://docs.rs/zeroize/1.9.1/zeroize/struct.Zeroizing.html) checked. This memory handling does not control OS swap or third-party IPC/HTTP buffers.

The 0.3.0 folder chooser (`tauri-plugin-dialog`) is exposed through a small trusted Rust command; no broad file-system permission is granted to web content. Four locked packages were added; the generated inventory contains 684 locked components. The official [Tauri dialog documentation](https://v2.tauri.app/plugin/dialog/) was checked on 2026-10-31.

Sources: [Tauri releases](https://v2.tauri.app/release/), [npm registry](https://registry.npmjs.org/), [crates.io API](https://crates.io/api/v1/crates), [official MCP Rust SDK](https://github.com/modelcontextprotocol/rust-sdk), [Node releases](https://nodejs.org/en/about/previous-releases). Providers are called over documented HTTPS directly; no additional Google SDK is needed.

## Audit

`npm audit`: no known vulnerabilities. `cargo audit`: no vulnerability entries; informational warnings are assessed separately:

- `ttf-parser` is unmaintained (RUSTSEC-2026-0192). In FrameStudio it only processes the bundled, hashed font, never imported fonts. Temporary exception; replace with the maintained `skrifa` during the next maintenance.
- `proc-macro-error 1.0.4` (build macro, RUSTSEC-2024-0370).
- `zbus_names 4.3.5` / `zvariant 5.15.1` / `zvariant_derive 5.15.1` (Linux keyring transitive, RUSTSEC-2026-0371/0370/0372) and `glib 0.18.5` unsound `VariantStrIter` (Linux webview transitive, RUSTSEC-2024-0429). These Linux paths are not built on macOS/Windows. Linux is not a shipped or accepted platform; its platform dependencies must be renewed before adding Linux support.

No blanket ignore list for vulnerabilities is configured.

## Build

```sh
npm ci
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --locked --no-default-features
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets -- -D warnings
npm run tauri -- build
```

A macOS build requires Xcode Command Line Tools, Rust and Node. The result is in `src-tauri/target/release/bundle/macos/FrameStudio.app`, the DMG in `bundle/dmg`. The installed app needs none of these developer tools. The local bundle is ad-hoc signed via `signingIdentity: "-"`, not signed with an Apple Developer ID or notarized.

On Windows, build the installer with `npm run tauri -- build --bundles nsis`; see [WINDOWS.md](WINDOWS.md).

CI actions are pinned to full commit SHAs that were verified on 2026-10-31 through GitHub's official release/commit API: checkout 7.0.1, setup-node 7.1.0, upload-artifact 7.0.2 and a fixed rust-toolchain commit. Dependabot proposes updates for npm, Cargo and GitHub Actions; updates are never merged automatically.
