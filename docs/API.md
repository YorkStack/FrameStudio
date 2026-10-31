# REST und MCP

In Einstellungen REST bzw. MCP einschalten. Automatisierte kostenpflichtige Aufträge brauchen zusätzlich die explizite Ausgabenfreigabe. Standardmäßig ist alles aus. Testprovider-Aufträge erzeugen ausschließlich lokale Testbilder.

## Headless

macOS-Binary: `FrameStudio.app/Contents/MacOS/framestudio`.

```sh
"/path/to/FrameStudio.app/Contents/MacOS/framestudio" --serve
```

Startet REST und den persistenten Worker ohne Fenster. `--data-dir /path/to/store` muss bei GUI/CLI/MCP übereinstimmen, wenn ein abweichender Datenbestand verwendet wird. Mehrere Prozesse dürfen denselben Bestand öffnen; die Workersperre serialisiert Requests. Laufende GUI aktiviert REST selbst. Bei belegtem Port nicht eine zweite REST-Instanz starten; MCP kann parallel laufen.

`--rpc` liest einen lokalen JSON-Auftrag von stdin und gibt dessen JSON-Ergebnis aus. Dieser Modus ist vertrauenswürdige Administration, etwa um Einstellungen vor einem Headless-Start zu konfigurieren. Provider-Schlüssel niemals als Kommandozeilenargument oder in Skripte schreiben.

## REST

Unter Einstellungen → Zugänge & Zugriffstokens einen benannten REST-Zugang erzeugen, Empfänger und Zweck vermerken, den einmalig angezeigten Token kopieren und sicher beim Client hinterlegen. `scripts/rest-example.py` fragt es verdeckt ab und listet Projekte. Der Server akzeptiert ausschließlich Host `127.0.0.1:43120`, Bearer-Token und keinen Origin-Header. Er ist absichtlich kein Browser-Webservice. Einzelwiderruf in Einstellungen wird bei jedem weiteren Zugriff geprüft; andere Zugänge bleiben gültig.

- GET `/v1/projects`
- GET `/v1/jobs/{id}`
- GET `/openapi.json`
- POST `/v1/operations/{operation}` mit JSON-Argumentobjekt
- POST `/rpc` mit `{ "op": "project.list", "args": {} }`

Die vollständigen Operationen/Payloads stehen in [CONTRACT.md](CONTRACT.md); sie gelten für GUI, REST und MCP identisch. Schema zusätzlich per `framestudio --openapi` offline. Fehler: HTTP 400 mit `error`; nicht autorisierte Zugriffe: 401. Bild-/Exportbytes kommen als `data_base64` und `mime`, nicht nur als unzugänglicher Pfad.

Ablauf: `project.save` → `style.list` → `character.save` → `asset.import` → `prompt.preview` → `cost.estimate` → `job.submit` (eindeutiger `idempotency_key`) → `job.get`/`job.list` → `asset.list`/`asset.read` → `export.create`/`export.read`. Die unveränderte Idempotenz-ID für einen Wiederholungsversuch verhindert einen zweiten lokalen Start. Änderungen am Auftrag brauchen eine neue ID. Das ist keine Exactly-once-Zusage des Providers.

## MCP stdio

Offizielles Rust-SDK `rmcp`, kein selbst gebauter JSON-RPC-Ersatz. Konfiguration für einen lokalen MCP-Host (Pfade an die Installation anpassen):

```json
{"mcpServers":{"framestudio":{"command":"/path/to/FrameStudio.app/Contents/MacOS/framestudio","args":["--mcp"]}}}
```

Der lokale Host muss `FRAMESTUDIO_MCP_TOKEN` an den MCP-Kindprozess weitergeben. Dazu in FrameStudio einen Zugang mit Bereich **MCP** oder **REST & MCP** anlegen. Die Konfigurationsdatei oben enthält absichtlich kein Geheimnis; ohne gesetzte Variable schlägt der Start verständlich fehl. Keine Provider-Schlüssel als lokale Zugriffstokens verwenden. Die aktive Betriebssystem-Sitzung stellt separat den Provider-Keychain-Zugang her. Werkzeuge ersetzen den Punkt im Operationsnamen durch Unterstrich, z.B. `job_submit`. `asset_read` liefert native MCP-Bildinhalte und strukturierte Metadaten, `export_read` die vollständigen Dateibytes. Lokaler ausführbarer SDK-Client:

```zsh
# Token verdeckt eingeben; steht weder in der Kommandozeile noch im Shell-Verlauf.
read -rs 'FRAMESTUDIO_MCP_TOKEN?MCP-Zugriffstoken: '
print
export FRAMESTUDIO_MCP_TOKEN
cargo run --manifest-path src-tauri/Cargo.toml --no-default-features --example mcp_client -- /path/to/framestudio
unset FRAMESTUDIO_MCP_TOKEN
```

Für einen eigenen MCP-Host dessen Geheimnis-/Umgebungsvariablenverwaltung verwenden. Ein über Finder gestarteter Host übernimmt keine Variablen aus einem Terminalfenster. Keine echten Tokens in Git, Beispiel-JSON oder Shellskripte schreiben. Der mitgelieferte SDK-Client erbt die Variable; der Server prüft den Bereich beim Start und den Widerruf bei jedem Werkzeugaufruf sowie `tools/list`. Ein abgelehnter Start startet keinen Worker.

Ein gehostetes Modell kann nicht direkt auf localhost oder stdio dieses Macs zugreifen. Ein eigener lokaler Agent startet den MCP-Prozess, ruft Werkzeuge auf und vermittelt bei Bedarf Ergebnisse an seine Modell-API. Er entscheidet dabei ausdrücklich, welche lokalen Inhalte er nach außen übermittelt. FrameStudio öffnet keinen Tunnel.

## Persistente Exportjobs

Für längere Exporte `export.submit` mit denselben Argumenten wie `export.create` aufrufen. Die Antwort enthält eine persistente Exportjob-ID; `export.get {id}` liefert `queued`, `running`, `completed` oder `failed`. Nach `completed` enthält `result.id` die ID für `export.read`. Die GUI nutzt diesen Ablauf. `export.create` bleibt als synchrone Komfortoperation verfügbar. `export.preview` zeigt die vom Kern bereinigten Dateinamen vorab. `--worker` betreibt nur den Verbraucher ohne HTTP-Port/Fenster. Exportaufträge sind lokal und lösen keine Providerkosten aus.

## OpenAI neben Gemini

Schlüssel getrennt in den App-Einstellungen hinterlegen. Für `cost.estimate`, `prompt.preview` und `job.submit` ist `provider: "openai"` verfügbar. Unterstützte Modell-IDs und Fähigkeiten zuerst über `state.models` lesen. Beispielargumente (Projekt-ID ersetzen):

```json
{"project_id":"PROJECT_UUID","description":"Eine technische Strichzeichnung einer Werkstatt","provider":"openai","model":"gpt-image-2.5-sunburst-2026-09-08","aspect_ratio":"3:2","resolution":"1K","quality":"medium","count":1,"reference_ids":[],"idempotency_key":"workshop-image-001"}
```

Die Angabe `1K` entspricht hier 1024×1024, 1536×1024 oder 1024×1536 je Seitenverhältnis. Referenzen verwenden bestehende Asset-IDs desselben Projekts. Automatisierte OpenAI-Aufträge benötigen dieselbe vorherige Ausgabenfreigabe und atomare Budgetreservierung wie Gemini. Keine Schlüssel im Auftrag übergeben. Die obigen Argumente allein lösen keinen Auftrag aus; dafür ist `job.submit` erforderlich.

## Stilvorschauen über REST/MCP

`style.preview.prepare {id}` bereitet einen Stil und sein gewöhnliches Vorschauprojekt vor. Anschließend die gelieferten Felder mit `style_id: style.id` und `project_id: project.id` über `prompt.preview` und `cost.estimate` prüfen und erst mit `job.submit` starten. Nach Abschluss `asset.list` mit `style_id` aufrufen und mit `style.preview.set {id,asset_id}` ausdrücklich das gewünschte Ergebnis auswählen. MCP-Namen: `style_preview_prepare` und `style_preview_set`. Das Vorbereiten und Zuordnen kostet nichts; die Erzeugung ist ein normaler budgetierter Providerauftrag und erfordert bei Automation weiterhin eine lokale Ausgabenfreigabe.


## Preise und Claude-Prompt-Assistent

- `pricing.catalog {}` liefert Anbieter, Modelle, Preisstände, Quellen, Prüfdatum und klar als Ausgabeanteil gekennzeichnete Bildpreisbeispiele. Claude hat `capability: prompt_assistance` und `no_image_generation: true`.
- `pricing.refresh {provider?: "gemini"|"openai"|"anthropic"}` prüft öffentliche Quellen; ohne Auswahl alle drei. Keine Modellanfrage, keine Schlüssel und keine frei wählbare URL. Anbieterstatus `error` bedeutet: bisherige Preise behalten. `updated` bestätigt eine geprüfte Preisquelle, keine erfolgreiche Modellgenerierung.
- `assistant.models {}` liefert unterstützte Textmodelle. `assistant.estimate {project_id,text,instruction?,provider?,model?}` liefert Mikro-USD, Warnungen und `price_version`. Standard ist Anthropic; der kostenlose Testassistent muss ausdrücklich mit `provider: "test"` gewählt werden.
- `assistant.submit` nimmt dieselben Felder plus `idempotency_key` und optional `expected_price_version` entgegen. Antwort `{jobs,reused}`. `job.get`/`job.cancel` gelten unverändert; fertiger Text steht in `result_text`, Jobart in `kind: prompt_assist`. Es entstehen keine Assets oder Folgejobs.
- `cost.project` enthält neben Gesamtkosten `by_kind.image_generation` und `by_kind.prompt_assist`. Beide belasten dieselben atomaren Startgrenzen. Automatisierte Anthropic-Aufträge brauchen vorherige Ausgabenfreigabe, auch wenn `provider` im Auftrag ausgelassen wurde.

MCP-Namen ersetzen Punkte durch Unterstriche, beispielsweise `assistant_estimate` und `pricing_refresh`. Beispiel für einen **lokalen, kostenlosen** Texttest (Projekt-ID ersetzen):

```json
{"op":"assistant.submit","args":{"project_id":"PROJECT_UUID","text":"Ein Retro-Roboter in einer Werkstatt.","instruction":"Komposition genauer beschreiben.","provider":"test","idempotency_key":"prompt-test-001"}}
```

Bei Bild- und Textaufträgen nach der Schätzung deren `price_version` als `expected_price_version` mitsenden. Der Kern weist einen zwischenzeitlich geänderten Preis vor der Reservierung zurück. Wiederholte identische Aufträge behalten denselben Idempotenzschlüssel. Schlüssel weiterhin ausschließlich lokal in den Einstellungen ablegen, niemals als REST-/MCP-Payload.


## Lokale Zugangsverwaltung

Nur vertrauenswürdige GUI/CLI-Administration, nicht über REST/MCP aufrufbar:

| Operation | Payload / Ergebnis |
|---|---|
| `tokens.list` | `{}` → Liste von Metadaten, niemals Token oder Hash |
| `tokens.create` | `{name,recipient,purpose,scope}` → `{token,access,notice}`; `scope`: `rest`, `mcp`, `both`; Token einmalig |
| `tokens.update` | `{id,name?,recipient?,purpose?}` → Metadaten; Bereich und Erstellung nicht nachträglich veränderbar |
| `tokens.revoke` | `{id}` → `{revoked:true,count}`; nur diesen Zugang sperren; Verlauf bleibt |

Metadaten: `id`, `name`, `recipient`, `purpose`, `scope`, `created_at`, `last_used_at`, `revoked_at`, `active`. Zeiten stammen vom lokalen System, neue Werte sind RFC3339/UTC. Alte SQLite-Erstellungszeiten sind ebenfalls UTC. Letzte Nutzung bedeutet erfolgreiche Zugangsauthentifizierung, nicht erfolgreichen Abschluss eines Jobs. Empfänger ist eine selbst gepflegte Zuordnung, kein überprüftes Benutzerkonto.

Kompatibilität: Bestehende REST-Tokens bleiben gültig und erscheinen als „Bestehender API-Zugang“. Sie lassen sich nachträglich beschriften. `tokens.create {}` bleibt für lokale Skripte möglich (REST), `tokens.revoke {}` widerruft weiterhin alle aktiven Zugänge. Die GUI verwendet ausschließlich den Einzelwiderruf. **MCP verlangt ab dieser Version einen passenden Token**; ältere tokenlose MCP-Konfigurationen müssen um die oben erklärte Host-Umgebung ergänzt werden.

Widerruf verhindert neue Operationen, beendet aber keinen bereits angenommenen Auftrag und holt bereits abgerufene Daten nicht zurück. Jobs bei Bedarf zusätzlich abbrechen. Tokenrechte umfassen die zugelassenen Projektoperationen des gewählten Transportwegs; kostenpflichtige Automation braucht weiterhin ihre gesonderte lokale Freigabe und Budgets. Derselbe OS-Benutzer kann die lokale CLI administrieren: Tokens sind Client-Zugangsverwaltung, keine Isolation gegen den Besitzer der lokalen Dateien.

Nach dem Update alle alten GUI-/Headless-/MCP-Instanzen nach Abschluss laufender Jobs beenden und mit dem neuen Binary starten. Bereits laufende ältere Versionen kennen die neue Tokenverwaltung nicht; für deren Verhalten gelten die neuen Prüfungen nicht. Die App startet oder beendet keine fremden Nutzerprozesse automatisch.

## Eine Freigabe für eine vollständige Serie

Für Serien **`series.submit` / `series_submit`** verwenden. Der Client reicht Beschreibung, Projekt, stabile Idempotenz-ID und die konkrete Liste aller Bilder gemeinsam ein. Antwort: Serien-ID, Status `pending`, authentifizierte Herkunft, Bildanzahl und Gesamtschätzung in Mikro-USD. Noch kein Job wird verarbeitet und kein Budget reserviert. In der laufenden GUI oben **Serienfreigaben** öffnen, Zusammenfassung/Bildliste prüfen und **Gesamte Serie freigeben** wählen. Die Freigabe gilt für alle eingefrorenen Bilder; eine erneute Bestätigung pro Bild ist nicht nötig. Sie erteilt keine allgemeine Ausgabenfreigabe für spätere Aufträge.

`series.get {"id":"…"}` / `series_get` liefert danach `job_ids`, alle Jobs und `job_status_counts`; `asset.list/read` liefern die Ergebnisse. `series.cancel` stoppt wartende Bilder und fordert den bestmöglichen Abbruch bereits laufender Bilder an. `series.approve/reject` sind ausschließlich lokale Administrationsoperationen, keine REST-/MCP-Werkzeuge. Ohne GUI bleibt ein Auftrag pending; der lokale Besitzer kann nach Prüfung über vertrauenswürdiges `--rpc` freigeben. Eine gehostete Modell-API kann diese lokale Freigabe nicht selbst erteilen.

Herkunft: Transport REST/MCP plus ID, Name, Empfänger und Zweck des tatsächlich authentifizierten Tokens. Die Beschriftung dokumentiert den Zugang, keine verifizierte Personenidentität oder das konkrete Programm hinter einem geteilten Token. Der Client kann kein `origin`-Feld einschleusen. Änderungen am Tokenlabel ändern historische Herkunft nicht. Widerruf blockiert neue Schnittstellenaufrufe; bereits eingereichte/freigegebene Serien bei Bedarf separat ablehnen/abbrechen.

Grenzen: maximal32 Bilder nach Auflösung von `count`/`variants`, maximal100 offene Freigaben. Ein Projekt pro Serie; je Bild dürfen Stil, Charakter, Referenzen, Prompt, Provider und Modell abweichen. Motive, Referenzen und Promptbausteine bleiben eingefroren. Vor der lokalen Freigabe darf der Nutzer Anbieter, Modell, Qualität und Auflösung für die gesamte Serie wählen. Nicht unterstützte Seitenverhältnisse oder Referenzen blockieren diese Auswahl, statt Inhalte still anzupassen. Der eingereichte Stand bleibt unter `submitted_items` erhalten, die freigegebenen Snapshots stehen unter `items`. Inhaltliche Änderungen benötigen einen neuen Auftrag/neuen Idempotenzschlüssel. Preisänderungen nach der angezeigten Vorschau blockieren den Start bis zur erneuten Prüfung. Beim Freigeben werden die gesamte Auftragsgrenze und das aktuelle Projektbudget in derselben SQLite-Transaktion wie alle Jobstarts geprüft/reserviert. Kosten sind Schätzungen und keine garantierte Providerobergrenze. Ein Fehler wird nicht automatisch wiederholt; übrige freigegebene Bilder laufen weiter. Erfolgreiche Wiederholung einzelner Bilder ist ein neuer Auftrag.

**Kompatibilität:** `job.submit` bleibt für direkte Aufträge bestehen und benötigt bei kostenpflichtiger Automation weiterhin die vorhandene allgemeine lokale Freigabe. Für die hier beschriebene Freigabe pro Serie `series.submit` verwenden; die allgemeine Automationseinstellung kann ausgeschaltet bleiben. Sitzungsschlüssel gelten nur im jeweiligen Prozess. Wird eine Serie mit aktivem Sitzungsschlüssel freigegeben, beansprucht nur ein Worker mit aktivem Sitzungsschlüssel für den jeweiligen Provider diese Jobs; ein MCP-/Headless-Prozess ohne diesen Schlüssel überspringt sie. Nach einem Neustart bleiben die Bilder wartend, bis der Sitzungsschlüssel wieder eingegeben wird. Die Freigabe bleibt erhalten, der Schlüssel wird niemals zwischen Prozessen übertragen.

Ausführbare Beispiele (standardmäßig nur lokaler Testprovider): Projekt-ID in `examples/series-request.json` einsetzen. Für echte Bilder Provider/Modell ausdrücklich im JSON setzen; die lokale Serienfreigabe bleibt erforderlich.

```sh
python3 scripts/rest-series.py examples/series-request.json
python3 scripts/rest-series.py --status SERIEN-ID
# MCP-Token wie oben sicher an den lokalen Host übergeben:
cargo run --manifest-path src-tauri/Cargo.toml --locked --no-default-features --example mcp_series_client -- /path/to/framestudio examples/series-request.json
```

Keine Provider-Schlüssel oder Zugriffstokens in die JSON-Datei schreiben. Idempotenz gilt pro Transport und Zugang: dieselbe ID mit identischem Inhalt liefert dieselbe Serie, auch nach einem Neustart oder einer verlorenen Antwort. Das erneute Absenden einer abgelehnten Serie öffnet sie nicht erneut.


## Modelle vor einer Serienfreigabe vergleichen

`series.quote {id, selection?: {provider, model, resolution, quality?}}` (MCP `series_quote`) berechnet eine vollständige neue Kostenvorschau ohne Provideraufruf, Job oder Reservierung. Ohne `selection` bleibt die Modellauswahl der eingereichten Bilder erhalten. Mit Auswahl gilt sie für alle Bilder. Antwort: `items` mit unveränderten Prompts/Referenzen, `estimated_microusd`, `count`, `currency`, `quote_id`, allgemeine `job_limit_microusd`. Qualitätsstufen und Auflösungen aus `state.models` verwenden. Nur vorhandene Fähigkeiten sind erlaubt; keine automatische Formatänderung.

Die GUI zeigt Modelle auch bei fehlendem Schlüssel ausgegraut mit Schätzungen. Pro Anbieter gibt es derzeit einen gespeicherten Schlüssel und optional einen vorrangigen Sitzungsschlüssel, keine Auswahl mehrerer Konten desselben Anbieters. Key-Status ist keine bestätigte Providerberechtigung; vor Freigabe wird der lokale Lesezugriff geprüft.

Lokal: `series.approve {id, selection?, expected_quote}` berechnet dieselbe Vorschau erneut in der Starttransaktion. Eine abweichende `quote_id` verhindert den Start. Anbieteränderung ohne `expected_quote` wird abgewiesen. Die Originalanfrage und authentifizierte Herkunft bleiben erhalten; wiederholtes Einreichen mit ursprünglicher Idempotenz-ID erstellt keinen zweiten Auftrag.

Übersteigt die Serie die allgemeine Startgrenze, kann der lokale Nutzer ausschließlich für diese Serie eine zusätzliche Checkbox bestätigen. Dann enthält die Freigabe `series_limit_microusd` in exakt der geprüften Gesamthöhe. Das verändert keine globale Einstellung, hebt das Projektbudget nicht auf und ist über REST/MCP nicht freigebbar. Ohne diese ausdrückliche Auswahl gilt unverändert die allgemeine Startgrenze. Die Summe wird atomar für alle Bilder reserviert; Kosten bleiben Schätzungen, keine garantierte Rechnung.


Lokale Schlüsseldiagnose: `secret.check {provider, source?: "active"|"keychain"}`. Standard `active` nutzt einen vorhandenen Sitzungsschlüssel vor dem OS-Speicher. `keychain` prüft ausschließlich den Betriebssystem-Speicher, ohne einen Sitzungsschlüssel zu entfernen. Rückgabe nur Status; kein Klartext, kein Provideraufruf. Diese Operation ist über REST/MCP weiterhin gesperrt.

## Nur die Bilder einer Serie exportieren

`asset.list` akzeptiert zusätzlich `series_id` zusammen mit `project_id`. Die Antwort enthält ausschließlich gespeicherte Ergebnisse der Serienjobs in deren Reihenfolge, mit `total`, `offset` und `limit` (maximal 200 pro Seite). Importierte Referenzen und fremde Serien sind ausgeschlossen; eine Serie eines anderen Projekts wird abgelehnt. Alle Seiten abfragen und die geordneten `items[].id` als `asset_ids` an `export.submit` übergeben. Bei leerem Ergebnis keinen Export starten: Eine leere Exportauswahl bedeutet weiterhin „ganzes Projekt“. `export.get` liefert den Status; `export.read` liefert die Datei oder bei mehreren Dateien ein ZIP als Base64. Keine erneute Generierung und keine Providerkosten.

## Ergänzungen 0.2.0

`project.prompt.save`: `{id, prompt_template: {scene, character, instruction}}` speichert die drei Editor-Vorgaben; erlaubt sind ausschließlich diese Textfelder (je 8000 UTF-8-Bytes). `project.save` akzeptiert dieselbe Vorlage. Bei automatisierten Generierungen weiterhin die gewünschten `blocks.scene`, `blocks.character` und `description` ausdrücklich mitgeben; die Vorlage ersetzt keine Requestparameter.

`assistant.estimate` / `assistant.submit` akzeptieren `target: scene | character | instruction | all` (Standard: instruction). `all` liefert nach Validierung `result_sections` mit den drei Feldern; unstrukturiertes/ungültiges Ergebnis wird als Fehler behandelt und nicht automatisch angewendet. Modellantworten bleiben kostenpflichtige Textjobs; Freigaben und Budgets gelten unverändert. `character.save` akzeptiert `portrait_asset_id`: erzeugtes Asset aus demselben Projekt, unabhängig von `reference_ids`. Leerer String aktiviert die automatische Portraitauswahl.

Der Projektpapierkorb und Dateisystem-SBOM-Export sind lokale Verwaltungsfunktionen und werden nicht als automatisierte REST-/MCP-Werkzeuge freigegeben.
