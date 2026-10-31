# FrameStudio

Local image workshop for macOS, built with Tauri, React, TypeScript, Rust and SQLite. German and English interface. Source code under the [MIT license](LICENSE).

## Download / Herunterladen

[Latest release: macOS Apple Silicon](https://github.com/YorkStack/FrameStudio/releases/latest)

Open the DMG and copy FrameStudio to Applications. No Docker, database server, Node or Rust installation is needed to use the app. This early release is **ad-hoc signed, not Developer ID signed or notarized**. macOS may block downloaded builds; no system security settings need to be disabled. Building the source locally is an alternative. Windows CI is provided; a Windows download will only be offered after a successful Windows build.

DMG öffnen und FrameStudio nach „Programme“ kopieren. Die Sprache lässt sich unter **Einstellungen → Darstellung → Sprache** umstellen. Die Vorabversion besitzt noch keine Apple-Notarisierung.

## English documentation

- [Quickstart with screenshots](docs/QUICKSTART.md) — connect a provider, create an image and export it.
- [User Guide](docs/USER-GUIDE.md) — projects, prompt sections, characters, whole-series export, budgets, workspace changes and access tokens.

[Download both guides for offline reading](https://github.com/YorkStack/FrameStudio/releases/download/v0.3.0/FrameStudio-0.3.0-English-Guides.zip) — HTML with screenshots; unzip and open `QUICKSTART.html`.

## First image / Erstes Bild

1. Open **Settings → API keys**. Select Gemini or OpenAI. The help section links to the provider's key creation page.
2. Explicitly save the key in the OS credential store, or activate it only for the current app session. The status distinguishes presence from verified local read access. Never paste a key into an issue or chat.
3. Choose **Create single image / Einzelbild erzeugen** in the sidebar. Enter a description, choose a style and optional reference image, review the estimate, then generate. Results are retained in the **Single images / Einzelbilder** project.
4. Use **Projects** for reusable scene, character and instruction blocks, image comparisons and character series. A series approval shows its source, exact image list and total estimate.
5. Export a selection or complete series as originals, PNG, JPEG, WebP, TIFF, offline HTML, PDF, PPTX, sprite sheet or project ZIP.

Gemini and OpenAI generate images. Anthropic Claude is an optional **text prompt assistant**, not an image generator. A clearly labeled deterministic test provider works without keys or paid requests. Model catalogs, supported formats and dated pricing sources are documented in [provider verification](docs/PROVIDER-VERIFICATION.md).

The release contains neutral style thumbnails, editable style templates and application resources. It contains **no developer API keys, access tokens, personal photos, user projects, generation history or project prompts**. User-created content stays in the chosen local workspace. Image generation sends only the selected prompt and references to the selected provider.

## Settings and local data

- **Appearance:** German/English, light/dark/system and simple/expert mode. User-written descriptions, saved historical prompts and provider-supplied details keep their original language.
- **Storage & export:** choose an empty directory or an existing FrameStudio workspace. The switch takes effect at the next app start. It does not move or delete the previous workspace; use project ZIP backup/restore to transfer projects.
- **REST, MCP & access:** active credentials are shown first; revoked grants are retained in a compact history. Active grants can be revoked individually. Provider API keys are separate from these local access tokens.
- **About & SBOM:** export the CycloneDX 1.6 dependency inventory. It describes the lockfiles, including build and cross-platform packages, not a binary vulnerability assessment.

Default data locations follow the OS application-data convention. macOS: `~/Library/Application Support/com.YorkStack.FrameStudio`. The workspace contains SQLite, original files, thumbnails and exports. Keys use the OS credential store or process memory only; no plaintext fallback. macOS app rebuilds can invalidate keychain authorization; see [keychain guidance](docs/KEYCHAIN.md).

Costs are shown in USD. Estimates, usage-derived amounts, provider-confirmed amounts and unknown costs are distinct. Local limits are not a global provider billing cap. No automatic paid retry after an uncertain result.

## REST and MCP

Both use the same Rust application core and budgets as the GUI. Interfaces are off by default. REST binds to loopback with bearer tokens and restrictive Host/Origin checks. MCP uses the official Rust SDK over stdio and scoped, revocable tokens.

See [API setup and executable examples](docs/API.md), [operation contracts](docs/CONTRACT.md) and [OpenAPI](docs/openapi.json). A hosted model cannot access localhost by itself; a local agent must operate the MCP client.

## Build and checks

Pinned versions: Node 24.22.1, npm 12.2.1 and Rust 1.98.1. Python 3.11+ is used only to build the SBOM. macOS builds require Xcode Command Line Tools; Windows builds require MSVC tools and WebView2.

```sh
npm ci
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --locked
cargo build --manifest-path src-tauri/Cargo.toml --locked --example session_ui_fixture
npx playwright install chromium
npm run test:e2e
npm run tauri -- build
```

macOS artifacts appear in `src-tauri/target/release/bundle/`. On Windows use `npm run tauri -- build --bundles nsis`. Lockfiles pin dependencies; binary hashes may differ between build environments and signing identities.

For public builds, remove local paths from compiler diagnostics:
```sh
RUSTFLAGS="--remap-path-prefix=$HOME=/build" npm run tauri -- build --ci
```

See [architecture](docs/ARCHITECTURE.md), [security](docs/SECURITY.md), [dependencies](docs/DEPENDENCIES.md) and [release validation](docs/RELEASE-0.3.0.md). Existing third-party licenses remain in force; bundled notices and the Noto Sans OFL license are in `src-tauri/resources/`.
