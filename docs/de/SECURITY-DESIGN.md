# Sicherheitsdesign

*English: [../SECURITY-DESIGN.md](../SECURITY-DESIGN.md) · Schwachstelle melden: [SECURITY.md](../../SECURITY.md)*

Dieses Dokument beschreibt die sicherheitsrelevanten Designentscheidungen. Das allgemeine Vertrauensmodell (lokales OS-Konto, Schlüsselspeicher, REST-/MCP-Grenzen) steht in [ARCHITECTURE.md](ARCHITECTURE.md).

## Serienfreigaben

`series.submit` speichert eine unveränderliche konkrete Bildliste mit Prompts, Referenzidentitäten und Preisgrundlage. Herkunft wird serverseitig aus Transport und authentifiziertem Token-Datensatz ermittelt. Kein Schlüssel/Token im Serienrecord. Namen/Empfänger sind lokale Zuordnungen, keine verifizierten Identitäten; wer einen Token teilt, teilt dessen Herkunftsangabe. REST/MCP können keine Serien freigeben oder die allgemeine Ausgabenfreigabe ändern. Nur lokale GUI/vertrauenswürdige CLI dürfen approve/reject; Budgetprüfung und Jobanlage erfolgen gemeinsam atomar. Keine Selbstfreigabe über clientgesteuerte Felder oder importierte Projektpakete.

Idempotenz ist pro Zugang/Transport an den unveränderten Auftrag gebunden. Preisänderungen vor Freigabe verlangen erneutes Einreichen; spätere Stiländerungen verändern eingefrorene Prompts nicht. Beim Start werden alle Schätzungen gemeinsam reserviert. Änderungen, Wiederholungen und zusätzliche Bilder sind neue Aufträge. Widerruf eines Tokens hebt bestehende Serienfreigaben nicht rückwirkend auf; zusätzlich Serien abbrechen. Maximal 32 expandierte Bilder und 100 offene Freigaben begrenzen versehentliche Explosionen. Bei unbekannten Preisen kein Start ohne belastbare Reservierung.

Bei aktivem Sitzungsschlüssel zum Freigabezeitpunkt werden die zugehörigen Jobs als `session_key_required` markiert. Ein Worker ohne Sitzungsschlüssel für diesen Anbieter beansprucht sie nicht und fragt dafür auch keinen OS-Schlüssel ab. Die Kennzeichnung ist kein Geheimnis; Zugangsdaten bleiben im jeweiligen Prozess. Nach einem Neustart ist erneute Schlüsseleingabe nötig, keine erneute Serienfreigabe. Ein absichtlich im anderen lokalen Prozess gesetzter eigener Sitzungsschlüssel desselben Anbieters ist kein isolierter Benutzerbereich; FrameStudio vertraut weiterhin dem lokalen OS-Benutzer.

## Schlüsselzugriff und Sitzungsschlüssel

`state.key_status` meldet nur vorhandene Schlüsselbund-Metadaten; „configured“ ist keine Zusage über die Leseberechtigung oder Provider-Gültigkeit. `secret.check` ist eine ausdrücklich lokal ausgelöste Leseprüfung, vor GUI-Generierung und über die Schaltfläche „Schlüsselzugriff prüfen“. Sie liefert nur `provider`, `readable`, `provider_contacted: false`, niemals das Geheimnis. REST und MCP dürfen diese Operation nicht aufrufen. Keine Hintergrundprüfung liest Schlüssel zur Statusanzeige.

macOS-Fehler -25308 (Interaktion nötig), -25293 (Autorisierung abgelehnt), -25291 (Dienst nicht verfügbar) und -128 (abgebrochen) werden getrennt gemeldet. Ein entsperrter Schlüsselbund garantiert keine Freigabe eines vorhandenen Eintrags für ein neu gebautes App-Binary. FrameStudio öffnet weiterhin keine automatischen Schlüsselbunddialoge und verändert keine Zugriffslisten. Die App „Passwörter“ ersetzt diese App-Freigabe in der „Schlüsselbundverwaltung“ nicht. Quellen: [Apple: App-Zugriff](https://support.apple.com/en-gb/guide/mac-help/kychn002/26/mac/26), [Apple: Access Control Lists](https://developer.apple.com/documentation/security/access-control-lists), geprüft 31.10.2026.

Auch normale Bild- und Promptaufträge werden bei aktivem Sitzungsschlüssel nun als `session_key_required` gespeichert, nicht nur freigegebene Serien. Ein Worker ohne Sitzungsschlüssel für den Anbieter beansprucht diese Jobs nicht und fällt nicht auf den OS-Speicher zurück. Nach App-Neustart muss der Benutzer den Sitzungsschlüssel erneut eingeben. Die lokale GUI-Prüfung vor Einreihung ersetzt keine Provider-Validierung; zwischen Prüfung und Verarbeitung kann sich der Schlüsselzugriff ändern.

## Öffentliche Verteilung (0.3.0)

Quellcode und Releases entstehen aus einer ausdrücklichen Dateiliste, ohne lokale Datenbanken, persönliche Bilder, Token-Datensätze, private Beispiel-ZIPs oder Entwicklungshistorie. Öffentliche Binaries ersetzen lokale Compilerpfade. Die mitgelieferten Stil-Thumbnails enthalten keine Text- oder EXIF-Blöcke mit Prompts.

Der native Ordnerdialog wählt nur einen Pfad. Beim Übernehmen des Arbeitsbereichs werden absolute Pfade, Symlink-Grenzen, Verzeichnisinhalt und Schreibbarkeit geprüft. Der alte Arbeitsbereich bleibt erhalten; die Auswahl kopiert oder löscht nichts stillschweigend.

Die Hilfe zum Anlegen von Schlüsseln öffnet ausschließlich diese festen offiziellen Ziele:

- https://aistudio.google.com/api-keys
- https://platform.openai.com/api-keys
- https://platform.claude.com/settings/keys

Die Ziele wurden am 31.10.2026 gegen die offizielle Anbieterdokumentation geprüft. Ein Provider-Schlüssel wird nie in eine URL eingesetzt.
