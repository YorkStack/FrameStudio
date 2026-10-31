# Architektur und Sicherheitsmodell

FrameStudio 0.1 ist eine lokale Tauri-2-App: React rendert die Oberfläche, Rust besitzt Daten, Providerzugriff, Budgetregeln und Exporte. Das installierte Programm enthält SQLite und alle Exportencoder. Kein Node/Python/Docker/Office für den Betrieb nötig. CLI, REST und MCP laufen aus derselben ausführbaren Datei und nutzen `Core::call`; es gibt keinen alternativen Web-Datenbestand.

## Daten

Standard: Betriebssystem-Anwendungsdaten (`ProjectDirs`, macOS Library/Application Support, Windows LocalAppData). `--data-dir` wählt explizit einen anderen lokalen Bestand. Die Oberfläche zeigt den aktiven Pfad. Originale und Thumbnails liegen als Dateien neben SQLite. UUIDs sind stabil. SQLite-Transaktionen sichern Metadaten und Reservierungen; dateibasierte atomare Veröffentlichung verhindert teilweise sichtbare Bilder. Nicht bestätigte Providerrequests werden bei Wiederanlauf nicht blind wiederholt. Eine prozessübergreifende Workersperre verhindert parallele Verbraucher desselben Bestands.

Das Betriebssystem-Konto ist die lokale Vertrauensgrenze. Ein Prozess mit Vollzugriff auf dasselbe Benutzerkonto kann Dateien ändern; FrameStudio stellt keine Mehrbenutzer-Isolation bereit. Ein Datenverzeichnis sollte nicht gemeinsam über Netzwerkdateisysteme betrieben werden.

## Geheimnisse und Zugriffe

Gemini-, OpenAI- und Anthropic-Schlüssel liegen unter getrennten festen Accounts (`gemini-api-key`, `openai-api-key`, `anthropic-api-key`) im bisherigen Service `com.yorkstack.framestudio`, sofern ausdrücklich die dauerhafte Ablage gewählt wird. Alternativ gibt es seit31.10.2026 einen ausdrücklich aktivierten Sitzungsspeicher im Prozess. macOS verwendet native Keychain Services, Windows den Credential Manager (`keyring`). Kein Klartextfallback. Die GUI schreibt Schlüssel und erhält nur Status zurück. Keine Schlüssel in Projektmanifest, SQLite, Browserstorage oder URL. REST/MCP haben unabhängig zufällige, einzeln widerrufbare Tokens; nur SHA256-Prüfwerte und Zugangsmetadaten liegen in separater `access.sqlite3`. Der Klartext erscheint einmal bei Erstellung. Die Zugriffsdatenbank gehört niemals zum Projektbackup.

Die Statusanzeige liest niemals ein Passwort. macOS fragt nur Attribute ab; Windows nutzt `get_attributes`. Status unterscheidet `configured`, `missing` und `unavailable`. Vorhandensein bestätigt keine Gültigkeit beim Provider. Andere Plattformen melden ohne geprüften Metadatenpfad `unavailable`.

Seit der Dialogkorrektur vom31.10.2026 deaktiviert `SecKeychainSetUserInteractionAllowed(false)` optionale Authentifizierungsdialoge vor dem ersten Keychain-Aufruf für den gesamten FrameStudio-Prozess. Zusätzlich enthalten alle `SecItem*`-Abfragen `kSecUseAuthenticationUIFail`; diese Abfrageoption allein deckt nicht alle klassischen Keychain-Zugriffe ab. Die Einstellung ist prozesslokal, ändert weder andere Apps noch dauerhafte Schlüsselbund-ACLs. Lesen, Schreiben/Ersetzen und Löschen verwenden native `SecItem*`-Operationen ohne vorheriges Passwortlesen. Bestehende Accounts bleiben erhalten. Gesperrte oder nicht freigegebene Einträge liefern Fehler; Entsperren und appspezifische Freigaben erfolgen bei Bedarf manuell durch den Nutzer. Keine automatische ACL-Erweiterung, Löschung oder Migration.

Ein gemeinsames Zugriffsgate verhindert parallele Operationen. Die GUI erhält nach spätestens2s für Status beziehungsweise8s für Lesen/Schreiben/Löschen einen Fehler. Ein noch im Betriebssystem blockierter Thread wird nicht gewaltsam beendet: Das Gate bleibt bis zu dessen tatsächlichem Ende belegt. Nach Timeout ist der Ausgang einer Schreiboperation unklar; keine automatische Wiederholung. Ein exklusiver Dateilock unter dem lokalen Cache `credential-access/operation.lock` verhindert außerdem konkurrierende Zugriffe neuer FrameStudio-Prozesse; er enthält keine Geheimnisse. Alte bereits laufende Builds kennen diese Sperre nicht und müssen vollständig beendet werden. Fehler und Fortschritt stehen direkt am Schlüsselfeld, ein synchrones UI-Gate verhindert Doppelklicks.

REST wird bewusst aktiviert, bindet ausschließlich 127.0.0.1:43120 und prüft den exakten Host samt Port. Origin-Header werden abgewiesen; keine CORS-Freigabe. Alle REST-Routen einschließlich Schema benötigen Token. MCP läuft als vom lokalen Client gestarteter stdio-Prozess, benötigt ebenfalls Aktivierung und bietet keine Schlüssel-/Token-/Einstellungsverwaltung. Externe Aufträge können sich keine eigene Ausgabenfreigabe erteilen. Der vertrauenswürdige lokale CLI-Administrationsmodus `--rpc` hat dieselben Rechte wie die GUI; keine untrusted Eingaben hineinleiten.

## Eingaben und Dateien

Importe übergeben Bildbytes, keine frei lesbaren Dateipfade oder URLs. Typ, Größe und Pixelbudget werden geprüft. Exportnamen werden sanitisiert und niemals als Shellbefehle interpretiert. ZIP-Inhalte werden auf relative Pfade, Symlinks, Eintrags-/Gesamtgrößen und Manifestversion kontrolliert. Exporte in isolierten neuen Verzeichnissen überschreiben keine Originale. React escaped Texte; HTML-Exporte escapen Texte und liefern eine restriktive CSP. Prompts, Kommentare und Providerantworten sind Daten.

Die Tauri-Webview hat keine Shell-/Filesystem-Plugins, keine entfernten Fonts/Scripts, keine Browser-Provideraufrufe. Nur der eigene RPC-Command ist erreichbar. CSP beschränkt Skripte auf lokale App-Ressourcen und verbietet Frames/Objekte/Formposts. Providerzugriff erfolgt aus Rust per TLS an feste Google- und OpenAI-Endpunkte ohne Redirects. OpenAI nutzt ausschließlich `https://api.openai.com/v1/images/generations` und `/v1/images/edits`; es gibt keine konfigurierbaren Fremd-URLs. Ein Wechsel des Anbieters sendet dessen eigenen Schlüssel.

## Kosten

Beträge sind ganzzahlige Mikro-USD. Eine Schätzung ist keine Rechnung. Preisversion, Quellen und Usage werden am Job festgehalten. Fehlende Usage bleibt unbekannt. Projektkosten umfassen Requests auch dann, wenn keine sichtbaren Assets entstehen. Lokale Limits begrenzen Starts in dieser Installation, nicht das globale Providerkonto. Unklare Ergebnisse werden nicht automatisch wiederholt. Kostenpflichtige Extras sind ausgeschaltet.


## Preisquellen und optionaler Textassistent

`pricing.rs` prüft öffentliche fest begrenzte Herstellerquellen. Das persistente JSON-Dokument `records(kind=pricing,id=global)` ergänzt Schema1 ohne Migration; alte Datenbestände nutzen den geprüften mitgelieferten Katalog bis zur ersten Aktualisierung. Ein separater Prozesslock verhindert verlorene Updates, SQLite speichert den fertigen Katalog atomar. Jobs behalten ihren eigenen Preissnapshot.

`assistant.rs` kapselt ausschließlich Anthropics Text-API. `core.rs` nutzt für Bild- und Textjobs dieselbe Queue, Workersperre, Idempotenz und Budgettransaktion. Additive JSON-Felder `kind` und `result_text` brauchen keine neue Tabelle. Textvorschläge bleiben Daten; weder Antwort noch Prompt werden als Befehl ausgeführt. UI übernimmt Ergebnisse nur ausdrücklich. Das native Schlüsselmodul erhält einen eigenen Anthropic-Account, keine neue Webview-Berechtigung. Neue feste Netzwerktrustgrenzen: `api.anthropic.com` für ausdrückliche Textaufträge sowie öffentliche Hersteller-Dokumentationsquellen für Preisabrufe. REST/MCP können keine Ziel-URL oder Zugangsdaten dafür liefern.


## Benannte Zugänge und Migration der Zugriffsdatenbank

`tokens.rs` verwaltet `access.sqlite3`, getrennt von der Projektdatenbank. Migration von unversioniertem Bestand auf `user_version=1` erfolgt unter `BEGIN IMMEDIATE`: vorhandene Hashes und Erstellungszeiten bleiben erhalten, stabile IDs und Metadaten werden ergänzt. Neue Felder: id/name/recipient/purpose/scope/last_used_at/revoked_at. Ein eindeutiger Index schützt IDs. Widerruf ist ein dauerhafter Status und löscht die Dokumentation des Zugangs nicht. Speicherung mit FULL-Synchronisierung, Dateirechten0600 auf Unix und Ablehnung von Datenbank-/Sidecar-Symlinks.

256 zufällige UUID-Bits abzüglich Formatbits bilden den lokalen Bearer-Token; nur sein SHA256-Digest wird gespeichert. Name/Empfänger/Zweck sind reine Texte, SQL parameterisiert. Volltext wird einmal an die lokale Oberfläche gegeben, nie in Listen, Projektpakete, Logs oder localStorage. MCP nimmt den Bearer ausschließlich aus `FRAMESTUDIO_MCP_TOKEN` des lokalen Hosts, nie aus einer URL oder CLI-Option. Prüfung bei Start und jeder Werkzeugoperation; kein Workerstart bei abgewiesener Anmeldung. Das gemeinsame Dateisystem bleibt eine Vertrauensgrenze: derselbe OS-Benutzer hat weiterhin lokale Administrationsrechte.

.gitignore schließt zusätzlich die Runtime-Datenbanken `framestudio.sqlite3*` und `access.sqlite3*` samt Journals aus, auch bei versehentlich im Repository gewähltem Datenverzeichnis. Provider-Schlüssel bleiben unabhängig davon im Betriebssystem-Schlüsselspeicher; Zugangsdatenbank und Projektbackups enthalten keine Provider-Schlüssel.


## Ausdrückliche Sitzungsschlüssel

`secrets/session.rs` hält pro Anbieter maximal einen Key unter einem Mutex ausschließlich im Prozessspeicher. `provider_key_for` prüft diesen zuerst; bei Treffer erfolgt kein OS-Schlüsselzugriff. Das gilt für Gemini/OpenAI und den Claude-Textassistenten gleichermaßen. Ersetzen/Entfernen überschreibt die gespeicherte Zeichenfolge mittels `Zeroizing<String>`; die an Provideradapter zurückgegebene Kopie ist ebenso geschützt. IPC-, Webview- und HTTP-Bibliothekskopien sowie OS-Swap/Crash-Dumps sind dadurch nicht vollständig kontrollierbar; eine physische Ausschlussgarantie für sämtliche Speicherkopien wird nicht behauptet. Die Anwendung persistiert den Key nicht und gibt ihn nie zurück.

`secret.session.set`, `.remove` und `.status` sind lokale Administrationsoperationen, nicht Bestandteil der REST-/MCP-Whitelist. Setzen und Entfernen berühren den Schlüsselbund nicht. Status enthält nur Anbieter/Boolean; der allgemeine Zustand ergänzt `status=session`, `configured=true`, `store=Session memory`. Keine Datenbankmigration. Nach Prozessende gibt es keinen Wiederherstellungspfad. Ein separater MCP-/Headless-Prozess erhält keine Kopie. Bereits laufende Requests behalten ihre eigene kurzfristige Kopie; Entfernen ist kein Providerabbruch. Budget-/Automatisierungsregeln bleiben unverändert.

## 0.2.0: Promptvorlagen und Bibliotheken

Additive Felder in bestehenden JSON-Records, keine neue SQLite-Tabelle oder Schema-Migration: Projekt `prompt_template`, `deleted_at`; Charakter `portrait_asset_id`; Stil `preview_hidden`; KI-Job `result_sections` und Snapshotparameter `target`. ZIP mit Metadaten erhält diese fachlichen Felder; ID-Neuzuordnung berücksichtigt Charakterportraits. Historische Prompts bleiben unverändert.

Papierkorboperationen laufen transaktional und verweigern Löschen bei offenen Jobs/Serienfreigaben. Daten und Kosten bleiben erhalten. Generation aus gelöschten Projekten wird abgewiesen. Erzeugte Stil-Kandidaten und automatische Charakterportraits werden aus Asset-Snapshots ermittelt; Importfotos sind keine automatischen Portraits. Bibliotheken aktualisieren ihren Zustand alle zwei Sekunden.

Elf bereits erzeugte Standardstil-Vorschauen liegen als statische PNG-Thumbnails im Frontend-Bundle; Manifest mit Stilversion, visuellen Bausteinen und SHA256. Sie erzeugen keine Assets, Requests oder Kosten in neuen Benutzerprojekten und werden nicht automatisch als Providerreferenz versendet.

SBOM wird deterministisch aus package-lock.json/Cargo.lock beim npm-Build erstellt und per include_str in den Kern eingebettet. Keine neuen App-Abhängigkeiten, Tauri-Berechtigungen, Providerziele oder CSP-Freigaben. Python 3.11+ ist nur Buildvoraussetzung; CI verwendet dieselbe npm-Buildstufe. Cargo-Lizenzangaben/Abhängigkeitsgraph und Betriebssystemframeworks sind noch nicht vollständig Bestandteil dieser Inventar-SBOM.
