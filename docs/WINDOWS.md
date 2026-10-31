# Windows x64 build and installation

FrameStudio uses the same Rust core and React interface on Windows and macOS.
The Windows package is an NSIS installer for Intel/AMD 64-bit Windows. ARM64-native and 32-bit packages are not included.

## Install

Download the Windows `-setup.exe` from the GitHub release, run it, and choose English or German. Installation is per-user and normally needs no administrator rights. If Microsoft WebView2 is missing, the installer downloads and installs it; an internet connection is required for that step. No Node.js, Rust, Python, Docker or database server is needed to use FrameStudio.

This early Windows package has no Authenticode publisher signature. Windows or an organization's security policy may block it. Do not disable system protections; managed installations should be reviewed by the administrator.

Open **Settings → API keys** and add your own provider key. Persistent keys use the Windows credential store. Session keys exist only in the running process. The macOS Keychain is not used on Windows. Select a local workspace in **Settings → Storage & export**; its contents are not moved automatically when switching folders.

The English [Quickstart](QUICKSTART.md) and [User Guide](USER-GUIDE.md) also describe Windows workflows; their screenshots were captured on macOS.

## Reproduce the build

On Windows, install the pinned Node/npm/Rust versions listed in the README, Python 3.11 or later, and the Visual Studio C++ build tools and Windows SDK required by Tauri. Then run:

```powershell
npm ci
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --locked --no-default-features
cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets -- -D warnings
npm run tauri -- build --bundles nsis -- --locked
```

`src-tauri/tauri.windows.conf.json` is automatically merged on Windows. Output: `src-tauri/target/release/bundle/nsis/FrameStudio_0.3.0_x64-setup.exe`.

The manually dispatched [Windows installer workflow](../.github/workflows/windows-release.yml) runs the tests, a synthetic native credential write/read/replace/delete test, Clippy, dependency audit, packaging, silent installation, installed-executable REST/export/restart acceptance, and a native-window startup smoke test. It uploads the installer and evidence for review; it does not automatically publish a release.

A startup smoke test is not a complete interactive Windows GUI acceptance. No paid provider requests are made by this workflow. Existing cross-platform tests cover actual REST and MCP connections; the installed release executable is additionally checked through REST.

Packaging follows the official [Tauri Windows installer documentation](https://v2.tauri.app/distribute/windows-installer/), checked on 2026-10-31. No new runtime dependency, database migration, Tauri permission or CSP relaxation is required by the Windows packaging configuration.
