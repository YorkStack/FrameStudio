# Providerprüfung · 31. Oktober 2026

Die erste Version implementiert `gemini-nano-banana-2.1` (stabil) über die dokumentierte Gemini Interactions REST-API. Der Aufruf erfolgt ausschließlich an `https://generativelanguage.googleapis.com/v1beta/interactions`, mit `x-goog-api-key` im Header, ohne Redirects. Der Rust-HTTP-Client braucht kein Google-SDK. Lokaler Testprovider: `deterministic-v1`; seine Prüfbilder sind keine KI-Generierungen.

Die [offizielle Bilddokumentation](https://ai.google.dev/gemini-api/docs/image-generation) beschreibt Text- und Bildinputs, Bildbearbeitung und `response_format` mit Seitenverhältnis, Auflösung und MIME-Typ. FrameStudio fordert JPEG-Ausgaben ohne das optionale Feld `delivery` an, 1K/2K/4K, zehn verbreitete Seitenverhältnisse und höchstens acht ausgewählte Referenzen. Dies sind bewusst begrenzte App-Fähigkeiten; andere aktuell dokumentierte Modelle werden nicht als ungeprüfte Konfiguration angeboten. Seeds, separate negative Promptparameter, Transparenz, Suche und andere kostenpflichtige Tools werden nicht angeboten. Der Baustein „Zu vermeiden“ ist gewöhnlicher Prompttext. Identitätskonsistenz bleibt modellbedingt unsicher.

[API-Referenz](https://ai.google.dev/api/interactions-api): `input` enthält Text- und Inline-Bildblöcke. Die Antwort wird über `steps[].content[]` ausgewertet; nur überprüfte Bildbytes werden gespeichert. `store:false` vermeidet die zusätzliche Abrufspeicherung. Synchron gesendete Anfragen haben bei Verbindungsabbruch gewöhnlich keine bekannte Provider-ID; daher ist keine verlässliche Statusabfrage möglich. FrameStudio behält dann „Ergebnis unklar“ und die Reservierung, ohne Wiederholung. Abbruch hält wartende Jobs an; bereits gesendete Anfragen werden zu Ende gelesen und Ergebnisse gesichert.

## Preisgrundlage

[Google Developer API-Preisliste](https://ai.google.dev/gemini-api/docs/pricing), Standard, USD, geprüft 2026-10-31; Snapshot `google-standard-2026-10-31-v1`:

| Nutzung | USD je Million Tokens |
|---|---:|
| Text-/Bildeingabe | 1,50 |
| Text- und Denkausgabe | 7,50 |
| Bildausgabe | 30,00 |

Die dokumentierten Bildausgabetokens sind 1120 (1K), 1680 (2K), 3780 (4K). Die Vorabschätzung addiert konservative Text-/Referenz- und Denkbudgets, ist jedoch keine garantierte Obergrenze. Tatsächliche Kosten werden nur bei vollständigen Usage-Feldern inklusive Modalitäten berechnet. Fehlende bzw. widersprüchliche Usage bleibt unbekannt, auch bei Antworten ohne Bild. Berechnete Beträge sind keine bestätigten Rechnungen. Ganzzahl-Mikro-USD verhindern Gleitkommafehler; Untereinheiten werden aufgerundet. Historische Jobs behalten Prompt-, Referenz- und Preissnapshot.

## Grenzen der Abnahme

Verträge, ungültige Parameter, Usage-Berechnung und fehlende/ungültige Bildantworten sind lokal getestet. Eine ausdrücklich freigegebene Echtgenerierung mit Referenzfoto wurde am 31.10.2026 nach der unten dokumentierten delivery-Korrektur erfolgreich abgeschlossen (JPEG 1024×1024). Die Live-Usage war hinsichtlich Ausgabemodalitäten unvollständig; Kosten bleiben unbekannt. Andere Accounts, Quoten und Größen sind damit nicht live abgenommen. Keine API-Schlüssel in Chat oder Projektdateien eingeben.

## OpenAI / ChatGPT-Bildmodelle · Erweiterung vom 31.10.2026

Die OpenAI-Integration verwendet die Images API, nicht die ChatGPT-Weboberfläche. Verifiziert anhand der offiziellen [Bildanleitung](https://developers.openai.com/api/docs/guides/image-generation), [Generierungsreferenz](https://developers.openai.com/api/reference/resources/images/methods/generate), [Bearbeitungsreferenz](https://developers.openai.com/api/reference/resources/images/methods/edit) und Modellseiten für [Sunburst](https://developers.openai.com/api/docs/models/gpt-image-2.5-sunburst) und [Flare](https://developers.openai.com/api/docs/models/gpt-image-2.5-flare).

- Festgelegte stabile Snapshots: `gpt-image-2.5-sunburst-2026-09-08`, `gpt-image-2.5-flare-2026-09-08`.
- Text-zu-Bild: `POST https://api.openai.com/v1/images/generations`, JSON, Bearer-Authentifizierung. Mit Referenzen: `/v1/images/edits`, dokumentiertes JSON-Feld `images` mit eingebetteten Data-URLs. Keine externen Bild-URLs, Datei-Uploads zur dauerhaften Providerablage oder Redirects.
- App-Auswahl: 1024×1024 (1:1), 1536×1024 (3:2), 1024×1536 (2:3), unter der Auflösungsgruppe `1K`. Explizite Qualität `low`, `medium` (Standard), `high`, `xhigh`, `max`. Modellwechsel zeigt notwendige Formatänderungen an. Weitere API-Größen, Masken, Streaming und Transparenz sind in dieser Integration nicht freigeschaltet.
- Pro Request ein PNG-Bild; Serien bleiben einzelne persistente Jobs. Maximal acht ausgewählte Referenzen. TIFF wird ausschließlich für den Transport nach PNG konvertiert, das Original bleibt unverändert. Grenzen: 20 MiB je Data-URL, insgesamt 32 MiB Referenz-Data-URLs; Antwort maximal 32 MiB, dekodiertes Einzelbild maximal 20 MiB/40 Megapixel.
- Antwort enthält eingebettete `data[].b64_json`-Bildbytes. URL-only-Antworten werden nicht heruntergeladen. Kosten werden unabhängig vom Bilderfolg aus vollständig plausibler Usage ermittelt; fehlende/widersprüchliche Usage bleibt unbekannt. HTTP-Request-ID wird gespeichert, falls vorhanden; sie ist keine wieder abrufbare Job-ID. Keine automatische Wiederholung nach Timeout oder unklarem Ergebnis.

### OpenAI-Preissnapshot

[Offizielle Preisliste](https://developers.openai.com/api/docs/pricing), Standardverarbeitung in USD, geprüft 31.10.2026, Version `openai-images-standard-2026-10-31-v1`: Texteingabe 5 USD, Bildeingabe 8 USD, Bildausgabe 30 USD je Million Tokens. Batch-Rabatte und Caching werden für diese direkten synchronen Images-Aufrufe nicht angenommen.

Der Ausgabeschätzer übernimmt den offiziellen GPT-Image-2.5-Rechner der Bildanleitung: Qualitätsraster-Längskante 16/24/48/64/96; kurze Rasterkante proportional zum Bildformat mit Round-half-to-even; Tokens = aufgerundet(Rasterfläche × (2.000.000 + Bildpixel) / 4.000.000). Eingabetext wird lokal anhand UTF-8-Bytes und jedes Referenzbild pauschal mit 8192 Tokens veranschlagt. Diese Eingabeannahmen werden im UI ausdrücklich angezeigt und sind keine garantierte Obergrenze. Historische Jobs bewahren die Preisgrundlage. Usage-Berechnung nutzt `input_tokens_details.text_tokens/image_tokens`, `output_tokens` sowie konsistente Gesamtsummen. Ein daraus berechneter Betrag ist keine bestätigte Rechnung.

Schlüssel werden in den Einstellungen getrennt für Google Gemini und OpenAI abgelegt, ersetzt und gelöscht. Die bestehende Gemini-Keychain-Zuordnung bleibt unverändert. Ein ChatGPT-Login allein genügt nicht; benötigt wird ein OpenAI-API-Schlüssel für das eigene API-Konto. Berechtigte Pro-/Business-Abonnements können nach ausdrücklicher Kontoverknüpfung API-Guthaben bieten; Details und Berechtigung stehen in der offiziellen [Anleitung zu API-Guthaben](https://developers.openai.com/api/docs/guides/chatgpt-api-credits). FrameStudio aktiviert keine Abrechnung, Kontoverknüpfung oder Guthabenaufladung.

OpenAI-Live-Abnahme ist offen: kein echter Schlüssel benutzt und keine kostenpflichtige Generierung autorisiert. API-Verträge, Parser, Kosten, Schlüsseltrennung, Budget-/Automationsregeln und GUI werden lokal mit synthetischen Daten geprüft. Weitere Anbieter benötigen jeweils einen verifizierten Adapter; es gibt keinen beliebigen API-URL- oder Klartext-Key-Speicher.

Rechnernachweis: offizielles Asset `https://developers.openai.com/_astro/GptImageTokenCalculator.react.CxseQDUo.js`, SHA-256 `00a23a754e5b3102f35f0823c5cbaa40c0bfbdc68f99eea6ce8f40067badf8b9`, geprüft 31.10.2026. Tests gleichen alle drei App-Größen und fünf Qualitätsstufen gegen die Rechnerwerte ab; beispielsweise 1024²: 196/439/1756/3122/7024 Ausgabetokens.


## Anthropic / Claude als Textassistent · 31.10.2026

Die offizielle [Vision-Dokumentation](https://platform.claude.com/docs/en/build-with-claude/vision) erklärt ausdrücklich, dass Claude keine Bilder generiert oder bearbeitet. FrameStudio bietet deshalb keinen Claude-Bildprovider an. Der optionale Prompt-Assistent verwendet `claude-sonnet-5-5`, dokumentiert in der [Modellübersicht](https://platform.claude.com/docs/en/models/sonnet-5-5/overview): Textinput/-output, 2 USD Eingabe und 10 USD Ausgabe je Million Tokens im Standardtarif. Die Bildanbieterpreise sind damit nicht direkt vergleichbar.

Vertrag: `POST https://api.anthropic.com/v1/messages`, Bearer-Header, `anthropic-version: 2026-10-21`, `max_tokens: 4096`, `thinking: {type: between_tools}`, ohne Tools, Caching, Bildinputs oder Streaming. Quellen: [Messages API](https://platform.claude.com/docs/en/api/messages/create), [Authentifizierung](https://platform.claude.com/docs/en/api/overview#authentication). Diese Integration setzt einen auf einen Workspace beschränkten API-Schlüssel voraus; kontoweite Schlüssel ohne Workspace-Zuordnung werden nicht unterstützt. Der feste Systemtext sowie ausschließlich die im Dialog sichtbare Beschreibung und Änderungsanweisung werden gesendet. Keine impliziten Projekte, Stile oder Referenzen.

Separate native Schlüsselzuordnung `anthropic-api-key`. Keine gespeicherten Schlüssel im UI, REST/MCP oder in Exporten. Fester TLS-Endpunkt, Redirects aus, 15 Sekunden Verbindungs-/120 Sekunden Anfragezeitlimit, Antwortlimit 1 MiB. Fehlende Usage bleibt unbekannt; vollständige Usage wird aus dem historischen Preissnapshot berechnet. Refusal, abgeschnittene oder leere Antworten werden nicht als fertiger Vorschlag angeboten. Unklarer Ausgang wird ohne automatische Wiederholung gespeichert. Textjobs reservieren dasselbe Projektbudget wie Bildjobs; Kosten bleiben nach Art getrennt auswertbar.

Live-Abnahme offen: keine durch die Entwicklung ausgelöste kostenpflichtige Claude-Anfrage, keine Benutzercredentials gelesen. Lokaler deterministischer Textassistent ist ausdrücklich als Test gekennzeichnet.

## Manuelle Preisaktualisierung

Die Schaltfläche lädt ausschließlich öffentliche Standardtarife der oben genannten Quellen. Anthropic wird über die offizielle [Markdown-Preisliste](https://platform.claude.com/docs/en/about-claude/pricing.md) und [Vision-Fähigkeitsbeschreibung](https://platform.claude.com/docs/en/build-with-claude/vision.md) geprüft. Google/OpenAI verwenden ihre HTML-Preistabellen. Der OpenAI-Rechner wird zusätzlich gegen den dokumentierten SHA-256 geprüft, aber niemals ausgeführt. Bei Änderungen am Rechner ist ein geprüftes App-Update erforderlich.

Pro Quelle maximal 2 MiB, 20 Sekunden Anfrage-/7 Sekunden Verbindungszeitlimit, keine Redirects und nur sechs feste HTTPS-URLs. Parser verlangen eindeutige Modellzeilen, Standardtarif, USD/Token-Einheiten und plausible positive Werte. Version, Quellenhash und Prüfzeit werden gespeichert. Teilfehler bewahren den letzten gültigen Modellpreis und dessen Prüfdatum; das Datum des fehlgeschlagenen Versuchs ist getrennt. Unbekannte Modelle werden nicht automatisch hinzugefügt.

Der neue Katalog wird atomar gespeichert. Gleichzeitige Aktualisierungen sind pro Datenbestand gesperrt; während des Netzzugriffs bleibt keine SQLite-Transaktion offen. Preisänderungen gelten für neue Schätzungen und Jobs. Eine inzwischen veraltete GUI-Schätzung führt vor Start zu einem Fehler mit Aufforderung zur erneuten Prüfung. Historische Jobs und ihre Usage-Berechnungen behalten die eingebettete Preisgrundlage. Ein Update gleicht weder Kontorabatte noch Rechnungen, Guthaben, Steuern oder individuelle Vertragspreise ab.


## Korrektur der direkten Gemini-Ausgabe ·31.10.2026

Erneute Prüfung der [Interactions-API, ImageResponseFormat](https://ai.google.dev/api/interactions-api): `mime_type` erlaubt dort ausschließlich `image/jpeg`. Der ursprüngliche Adapter sendete fälschlich `image/png`; korrigiert auf JPEG plus explizite Inline-Ausgabe. PNG als Referenzinput ist weiterhin dokumentiert und zulässig, ebenso lokale PNG-Exporte. Modell-ID, Endpoint, Auflösung, Seitenverhältnis, Preisgrundlage und `store:false` bleiben unverändert. Das ursprüngliche Dokument war hinsichtlich PNG-Ausgabe falsch; die lokale Testsuite prüfte bisher keinen passenden Requestvertrag. Die neuen Vertragsprüfungen schließen diese Lücke.

Der Nutzer meldete eine HTTP400-Ablehnung. Das ungültige Ausgabeformat ist ein belegter Fehler und passt dazu; der genaue ursprüngliche Google-Antworttext wurde nicht gespeichert, deshalb ist dessen alleinige Ursächlichkeit nicht abschließend bestätigt. Neue Fehler enthalten HTTP-Code, bekannte standardisierte Statuscodes und fest formulierte Hinweise für erkannte Schlüssel-/Freigabe-/Abrechnungs-/Formatfehler. Keine Rohtexte, Prompts, Schlüssel oder unbekannten Metadaten aus der Providerantwort werden dafür persistiert. Kein automatischer Retry und keine zur Entwicklung gestartete kostenpflichtige Generierung.

## OpenAI-Ausgabeformat, Schlüssel und Diagnosen erneut geprüft · 31.10.2026

Die [Images-Generate-Referenz](https://developers.openai.com/api/reference/resources/images/methods/generate) bestätigt `output_format: png|jpeg|webp` (Standard PNG), Base64-Rückgabe in `data[].b64_json` und beide verwendeten datierten GPT-Image-2.5-Modelle. Das alte `response_format` ist für GPT-Image nicht unterstützt und wird von FrameStudio nicht gesendet. FrameStudio fordert weiterhin explizit PNG, ein Bild und einen undurchsichtigen Hintergrund an; das ist eine gültige App-Voreinstellung. JPEG/WebP/TIFF sind davon unabhängige lokale Konvertierungen. Der Parser kann echte PNG-/JPEG-/WebP-Antworten ohne Änderung der Originalbytes verarbeiten. Anders als beim zuvor korrigierten Gemini-Adapter wurde kein Ausgabeformatfehler gefunden.

Die [Edit-Referenz](https://developers.openai.com/api/reference/resources/images/methods/edit) erlaubt JSON mit `images[].image_url`, einschließlich Base64-Data-URLs. FrameStudio verwendet ausschließlich geprüfte lokale Bildbytes, keine externen Bild-URLs; höchstens acht Referenzen (bewusst unter dem Providerlimit16). TIFF-Eingaben werden vor dem Versand tatsächlich in PNG konvertiert. `POST /v1/images/generations` bzw. `/v1/images/edits`, API-Key nur als Bearer-Header; TLS, feste Ziele und ausgeschaltete Redirects bleiben erhalten. Ein lokaler Test baut beide echten Requesttypen mit einem künstlichen Schlüssel, prüft Header/Redaktion sowie das Fehlen des Schlüssels in URL und JSON und sendet sie nicht. Sitzungsschlüssel bleiben anbieterspezifisch im Prozess; die vorhandenen Trennungs-/Neustarttests wurden erneut ausgeführt. Keine Benutzercredentials gelesen oder getestet.

[OpenAI-Fehlercodes](https://developers.openai.com/api/docs/guides/error-codes) unterscheiden Guthaben, Projekt-/Organisationsausgabenlimits, Nutzungslimit, Rate Limits und Serverüberlastung. Die [Bildanleitung](https://developers.openai.com/api/docs/guides/image-generation#handling-blocked-requests-and-other-errors) beschreibt `image_generation_user_error` und `moderation_blocked`. FrameStudio wertet diese strukturierten Felder nun mit einer Allowlist aus: spezifischer Code vor allgemeinem Typ, ansonsten verständlicher HTTP-Fallback. Zusätzlich werden ausschließlich bekannte Parameternamen wie `output_format`, `size` oder `quality` angezeigt. Keine freien Fehlermeldungen, Parameterwerte, Moderationsdetails oder unbekannten Codes aus Providerantworten gelangen in den sichtbaren Fehlertext. Keine Diagnose anhand frei formulierter Servertexte; bei unbekannten Codes bleibt die Ursache entsprechend allgemein.

Request-ID und gemeldete Usage bleiben erhalten, tatsächliche Kosten ohne Usage unbekannt. Auch bei 429/5xx erfolgt kein automatischer kostenpflichtiger Wiederholungsversuch. Nicht jede429 ist ein kurzfristiges Rate Limit. Ein unbekannter Ausgang nach Server-/Transportfehlern bleibt ausdrücklich unklar. Die Diagnose ersetzt weder die Kontoberechtigungsprüfung beim Anbieter noch eine echte Live-Abnahme.

## Erweiterte Gemini-Diagnose · 31.10.2026

Der ursprüngliche generische HTTP400-Eintrag enthält keinen rekonstruierbaren Google-Fehlertext. Modell, Interactions-Endpoint, Inline-Referenz und JPEG-Ausgabe wurden erneut gegen die oben verlinkte offizielle Dokumentation geprüft; bisher ist keine weitere konkrete Parameterverletzung belegt. Die Diagnose wertet jetzt zusätzlich die kleingeschriebenen Interactions-Fehlercodes aus und zeigt Googles bereinigte Fehlermeldung sowie Feldverletzungen. Vor Persistenz werden der tatsächlich verwendete Schlüssel, Prompt und Referenz-Base64 einschließlich JSON-escaped Echos entfernt, zusätzlich typische Schlüssel-/Bearer-/URL-/lange Tokenformen und Steuerzeichen. Textlängen sind begrenzt. Keine Rohantwort in Logs, kein automatischer Retry, keine Änderung historischer Fehlereinträge. OpenAI behält seine bestehenden strukturierten Diagnosen.

### Nachgewiesene Ursache und Korrektur

Der ausdrücklich freigegebene Live-Versuch vom 31.10.2026 lieferte HTTP400 mit „Image delivery mode is not supported.“. Deshalb wird `response_format.delivery` jetzt weggelassen. Das optionale Feld ist zwar im generischen Schema aufgeführt, der verwendete Bild-Endpunkt lehnt es tatsächlich ab. Die offiziellen REST-Bildbeispiele kommen ohne dieses Feld aus. Referenz-Base64, JPEG-Wunschformat, 1:1/1K, Modell und store:false bleiben unverändert. Der Parser akzeptiert weiterhin nur eingebettete, validierte Bildbytes und lädt keine Antwort-URLs herunter. Die Vertragsprüfung verhindert die Wiedereinführung des abgelehnten Feldes. Die Ursache des beobachteten Fehlers ist damit nachgewiesen; erfolgreiche Live-Erzeugung nach der Korrektur ist separat zu prüfen.

Live-Erfolg bestätigt: Mit entfallenem delivery-Feld lieferte derselbe Modell-/Fotoauftrag ein gültiges JPEG mit 1024×1024 Pixeln. Keine weiteren Requestparameter geändert. Gemeldete Ausgabetokens (1618) stimmen nicht mit der Modalitätenliste (1120 Bildtokens) überein; Kostenberechnung bleibt deshalb konservativ unbekannt. Details und Nachweis siehe ACCEPTANCE.md.

### OpenAI-Liveprüfung am 31.10.2026

Der erste ausdrücklich freigegebene Fotoauftrag erreichte die Images-Edit-API, wurde jedoch mit HTTP429 / `credit_balance_exhausted` abgelehnt. Der macOS-Schlüsselbundzugriff funktioniert für den hinterlegten OpenAI-Schlüssel. Keine Bildgenerierung bestätigt; Usage fehlte, Kosten bleiben unbekannt. Keine Wiederholung. Frühere Hinweise „kein echter Schlüssel benutzt“ beschreiben den damaligen Stand und sind durch diese Liveprüfung überholt. Konto-Guthaben muss vom Nutzer in der OpenAI-API-Abrechnung geprüft werden.
