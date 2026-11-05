## What changes

<!-- One or two sentences. Link an issue if there is one. -->

## Checks

- [ ] `npm run build` passes (includes i18n check and SBOM)
- [ ] `cargo test --manifest-path src-tauri/Cargo.toml --locked --no-default-features`
- [ ] `cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets -- -D warnings`
- [ ] New UI texts exist in German and English (`src/i18n.ts`, `src/en.json`)
- [ ] User-visible changes are described in `docs/` (English) and `docs/de/` (German)
- [ ] No API keys, access tokens, personal images or local paths in code, tests, screenshots or commit messages
