
## Serienfreigaben

`series.submit` speichert eine unveränderliche konkrete Bildliste mit Prompts, Referenzidentitäten und Preisgrundlage. Herkunft wird serverseitig aus Transport und authentifiziertem Token-Datensatz ermittelt. Kein Schlüssel/Token im Serienrecord. Namen/Empfänger sind lokale Zuordnungen, keine verifizierten Identitäten; wer einen Token teilt, teilt dessen Herkunftsangabe. REST/MCP können keine Serien freigeben oder die allgemeine Ausgabenfreigabe ändern. Nur lokale GUI/vertrauenswürdige CLI dürfen approve/reject; Budgetprüfung und Jobanlage erfolgen gemeinsam atomar. Keine Selbstfreigabe über clientgesteuerte Felder oder importierte Projektpakete.

Idempotenz ist pro Zugang/Transport an den unveränderten Auftrag gebunden. Preisänderungen vor Freigabe verlangen erneutes Einreichen; spätere Stiländerungen verändern eingefrorene Prompts nicht. Beim Start werden alle Schätzungen gemeinsam reserviert. Änderungen, Wiederholungen und zusätzliche Bilder sind neue Aufträge. Widerruf eines Tokens hebt bestehende Serienfreigaben nicht rückwirkend auf; zusätzlich Serien abbrechen. Maximal32 expandierte Bilder und100 offene Freigaben begrenzen versehentliche Explosionen. Bei unbekannten Preisen kein Start ohne belastbare Reservierung.

Bei aktivem Sitzungsschlüssel zum Freigabezeitpunkt werden die zugehörigen Jobs als `session_key_required` markiert. Ein Worker ohne Sitzungsschlüssel für diesen Anbieter beansprucht sie nicht und fragt dafür auch keinen OS-Schlüssel ab. Die Kennzeichnung ist kein Geheimnis; Zugangsdaten bleiben im jeweiligen Prozess. Nach einem Neustart ist erneute Schlüsseleingabe nötig, keine erneute Serienfreigabe. Ein absichtlich im anderen lokalen Prozess gesetzter eigener Sitzungsschlüssel desselben Anbieters ist kein isolierter Benutzerbereich; FrameStudio vertraut weiterhin dem lokalen OS-Benutzer.

## Schlüsselzugriff und Sitzungsschlüssel (31.10.2026)

`state.key_status` meldet nur vorhandene Schlüsselbund-Metadaten; „configured“ ist keine Zusage über die Leseberechtigung oder Provider-Gültigkeit. `secret.check` ist eine ausdrücklich lokal ausgelöste Leseprüfung, vor GUI-Generierung und über die Schaltfläche „Schlüsselzugriff prüfen“. Sie liefert nur `provider`, `readable`, `provider_contacted: false`, niemals das Geheimnis. REST und MCP dürfen diese Operation nicht aufrufen. Keine Hintergrundprüfung liest Schlüssel zur Statusanzeige.

macOS-Fehler -25308 (Interaktion nötig), -25293 (Autorisierung abgelehnt), -25291 (Dienst nicht verfügbar) und -128 (abgebrochen) werden getrennt gemeldet. Ein entsperrter Schlüsselbund garantiert keine Freigabe eines vorhandenen Eintrags für ein neu gebautes App-Binary. FrameStudio öffnet weiterhin keine automatischen Schlüsselbunddialoge und verändert keine Zugriffslisten. Die App „Passwörter“ ersetzt diese App-Freigabe in der „Schlüsselbundverwaltung“ nicht. Quellen: [Apple: App-Zugriff](https://support.apple.com/en-gb/guide/mac-help/kychn002/26/mac/26), [Apple: Access Control Lists](https://developer.apple.com/documentation/security/access-control-lists), geprüft 31.10.2026.

Auch normale Bild- und Promptaufträge werden bei aktivem Sitzungsschlüssel nun als `session_key_required` gespeichert, nicht nur freigegebene Serien. Ein Worker ohne Sitzungsschlüssel für den Anbieter beansprucht diese Jobs nicht und fällt nicht auf den OS-Speicher zurück. Nach App-Neustart muss der Benutzer den Sitzungsschlüssel erneut eingeben. Die lokale GUI-Prüfung vor Einreihung ersetzt keine Provider-Validierung; zwischen Prüfung und Verarbeitung kann sich der Schlüsselzugriff ändern.

## 0.3.0 public distribution

Source and releases are prepared from an explicit file allowlist, without local databases, personal images, token records, private example ZIPs or development history. Public binaries remap local compiler paths. Included style thumbnails have no text/EXIF chunks containing prompts.

The native folder chooser selects a path only. Applying the workspace change validates absolute paths, symlink boundaries, directory contents and writability. The old workspace is preserved; selection does not silently copy or erase it.

Key creation help opens only these fixed official destinations:
- https://aistudio.google.com/api-keys
- https://platform.openai.com/api-keys
- https://platform.claude.com/settings/keys

These destinations were checked against the official provider documentation on 2026-10-31. No provider key is inserted into a URL.
