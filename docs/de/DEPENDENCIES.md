# Reproduzierbarer Build und Abhängigkeiten

*English: [../DEPENDENCIES.md](../DEPENDENCIES.md)*

Prüfdatum 2026-10-31. npm-Pakete über `npm view <name> version`, Rust über crates.io API (neueste nicht zurückgezogene Version ohne Vorabkennzeichen), anschließend Lockfiles. Keine automatischen Updates. Tauri 3 alpha bewusst ausgeschlossen.

| Baustein | Version | Zweck |
|---|---|---|
| Tauri Rust / Build | 2.12.5 / 2.7.4 | Native Webview, Bundles, kein eingebettetes Chromium |
| Tauri JS API / CLI | 2.12.4 | Typisierte IPC und Buildwerkzeuge |
| React / TypeScript / Vite | 19.3.1 / 7.0.2 / 8.3.5 | Lokale Oberfläche und reproduzierbarer Frontendbuild |
| Rust / Node / npm | 1.98.1 / 24.22.1 / 12.2.1 | Gepinnte Entwicklerwerkzeuge, Node LTS; nicht im installierten Betrieb erforderlich |
| rusqlite | 0.41.1 bundled | Transaktionales eingebettetes SQLite |
| rmcp | 3.8.0 | Offizielles MCP-SDK und echter stdio-Clienttest |
| reqwest / tokio / axum | 0.13.5 / 1.54.1 / 0.8.9 | TLS-Provider, asynchrone Jobs und Loopback-API |
| scraper | 0.29.0 | Inerter HTML-DOM-Parser für eindeutige Herstellerpreistabellen; führt kein entferntes JavaScript aus |
| keyring | 4.2.0 | Native OS-Schlüsselspeicher; kein Datei-Fallback |
| core-foundation / security-framework-sys (nur macOS) | 0.10.1 / 2.17.0 | Direkte Metadatenabfrage ohne Passwortlesezugriff; keyring nutzt auf Apple beim Standard-Attributzugriff intern einen Secret-Lesezugriff |
| image / zip / lopdf | 0.25.10 / 9.2.0 / 0.46.0 | Echte portable Bildencoder, ZIP und PDF ohne Hilfsruntime |
| ttf-parser | 0.25.1 | Metriken des ausschließlich mitgelieferten OFL-Schriftfonts für Unicode-PDF |
| lucide-react | 1.66.0 | Konsistente zugängliche UI-Symbole, keine entfernten Ressourcen |
| Playwright | 1.64.1 | Browser-Abnahme gegen echten Rust-Kern |

Die beiden macOS-Metadatenabhängigkeiten waren bereits transitiv in Cargo.lock vorhanden. Ihre bestehenden exakten Versionen bleiben für diese Fehlerkorrektur erhalten; keine Paketaktualisierung und keine zusätzliche Runtime. Die verwendeten Security-Framework-Konstanten und ihr Verhalten wurden am 2026-10-31 gegen das installierte offizielle macOS-SDK (`Security.framework/Headers/SecItem.h`) geprüft.

Weitere kleine Rust-Hilfen: serde/serde_json für Datenverträge, uuid für kollisionsfreie IDs, chrono für Zeitstempel, base64 für Bildtransport, fs2 für Prozesssperre, sha2/subtle für Hashes und Tokenprüfung, directories für plattformübliche Speicherorte; tempfile nur Tests. Exakte transitive Versionen in Cargo.lock/package-lock.json.

Quellen: [Tauri Releases](https://v2.tauri.app/release/), [npm Registry](https://registry.npmjs.org/), [crates.io API](https://crates.io/api/v1/crates), [offizielles MCP Rust-SDK](https://github.com/modelcontextprotocol/rust-sdk), [Node Releases](https://nodejs.org/en/about/previous-releases). Provider verwendet dokumentiertes HTTPS direkt und benötigt kein zusätzliches Google-SDK.

## Audit

`npm audit`: keine bekannten Schwachstellen. `cargo audit`: keine Vulnerability-Einträge; informative Warnungen sind gesondert zu bewerten. `ttf-parser` ist unmaintained (RUSTSEC-2026-0192); verarbeitet in FrameStudio nur den fest gebündelten, gehashten Font, keine importierten Fonts. Befristete Ausnahme; bei nächster Wartung durch gepflegtes `skrifa` ersetzen. Weitere transitive Plattform-/Buildwarnungen werden im Abnahmebericht benannt; Linux ist keine abgenommene Zielplattform.

## Build

```sh
npm ci
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --locked --no-default-features
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets -- -D warnings
npm run tauri -- build
```

macOS erfordert für den Build Xcode Command Line Tools, Rust und Node. Resultat unter `src-tauri/target/release/bundle/macos/FrameStudio.app`, DMG unter `bundle/dmg`. Installierte App benötigt diese Entwicklerwerkzeuge nicht. Das lokale Bundle wird über `signingIdentity: "-"` ad-hoc signiert, jedoch nicht mit einer Apple Developer ID signiert oder notarisiert; keine Empfehlung, Systemschutz auszuschalten.

Unter Windows entsteht der Installer mit `npm run tauri -- build --bundles nsis`; siehe [WINDOWS.md](WINDOWS.md).

CI-Actions wurden am selben Prüfdatum über GitHubs offizielle Release-/Commit-API verifiziert und auf vollständige Commit-SHAs fixiert: checkout 7.0.1, setup-node 7.1.0, upload-artifact 7.0.2, rust-toolchain stable Commit. Dependabot schlägt Updates für npm, Cargo und GitHub Actions vor; sie werden nie automatisch übernommen.

Weitere Auditwarnungen: proc-macro-error 1.0.4 (Build-Makro, RUSTSEC-2024-0370), zbus_names 4.3.5/zvariant 5.15.1/zvariant_derive 5.15.1 (Linux-Keyring-Transitive, RUSTSEC-2026-0371/0370/0372); glib 0.18.5 unsound VariantStrIter (Linux-Webview-Transitive, RUSTSEC-2024-0429). Diese Linux-Pfade werden auf macOS/Windows nicht gebaut. Keine Linux-Auslieferung/Abnahme; vor zusätzlicher Linux-Unterstützung müssen die Plattformabhängigkeiten erneuert werden. Keine pauschale Ignore-Liste für Vulnerabilities eingerichtet.


Sitzungsschlüssel 31.10.2026: `zeroize=1.9.1` wird zusätzlich direkt referenziert, um gespeicherte Sitzungsschlüssel und Provider-Lesekopien beim Freigeben zu überschreiben. Version bereits transitiv im Cargo-Lock vorhanden; keine neue Paketversion. [Offizielle API-Dokumentation](https://docs.rs/zeroize/1.9.1/zeroize/struct.Zeroizing.html) geprüft. Diese Speicherbehandlung ersetzt keine Kontrolle über OS-Swap oder fremde IPC-/HTTP-Puffer.

## 0.3.0

tauri-plugin-dialog 2.8.4 ergänzt den nativen Ordnerdialog. Sein Callback ist über einen kleinen vertrauenswürdigen Rust-Befehl erreichbar; Webinhalte erhalten keine allgemeine Dateisystemberechtigung. Vier gesperrte Pakete kamen hinzu; das erzeugte Inventar enthält 684 gesperrte Komponenten. Die offizielle [Tauri-Dialog-Dokumentation](https://v2.tauri.app/plugin/dialog/) wurde am 31.10.2026 geprüft. Die Lizenzhinweise Dritter werden mit der Anwendung installiert.
