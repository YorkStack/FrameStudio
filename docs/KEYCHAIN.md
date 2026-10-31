# Schlüsselbunddiagnose · 31.10.2026

Die vorhandenen Provider-Einträge wurden ausschließlich über ihre Metadaten gefunden. Keine Geheimnisse wurden angezeigt oder exportiert.

| Schlüsselbund | Dienst/alter Anzeigename | Account | Angelegt (Europe/Berlin) |
| --- | --- | --- | --- |
| Anmeldung (`login.keychain-db`) | `com.yorkstack.framestudio` | `gemini-api-key` | 31.10.2026 11:12:44 |
| Anmeldung (`login.keychain-db`) | `com.yorkstack.framestudio` | `openai-api-key` | 31.10.2026 15:26:30 |

In **Schlüsselbundverwaltung** unter **Anmeldung → Alle Objekte** nach `com.yorkstack.framestudio` suchen. Die App **Passwörter** zeigt diese generischen App-Einträge nicht wie Website-Passwörter an. Neue explizite Speichervorgänge erhalten den Anzeigenamen `FrameStudio · Google Gemini API-Schlüssel` bzw. `FrameStudio · OpenAI API-Schlüssel`; Dienst und Account bleiben kompatibel.

Schreiben grundsätzlich funktionsfähig: isolierter synthetischer OS-Test hat Schreiben, Lesen, Ersetzen und Löschen erfolgreich geprüft. Zusätzlich den neuen Speichern-und-direkt-Lesen-Pfad mit einem ausschließlich synthetischen Eintrag geprüft. Die produktiven Einträge wurden nicht geändert oder gelöscht. Metadatenexistenz beweist nicht, dass ein aktueller App-Build das Geheimnis lesen darf.

Die App ist ad-hoc signiert. `codesign -d -r-` zeigt eine an den konkreten cdhash gebundene Designated Requirement; alte und neue App haben verschiedene Hashes. `security find-identity -v -p codesigning` meldet keine gültige Signieridentität. Zusammen mit dem erfolgreichen alten OpenAI-Zugriff und dem −25293 im neuen Build ist eine durch Updates veränderte App-Identität die wahrscheinliche Ursache. Die ACL selbst wurde nicht verändert; deren exakter Inhalt ist hier nicht abschließend geprüft.

## Einmalige Freigabe für einen konkreten Build

Nach Abschluss laufender Jobs die tatsächlich verwendete FrameStudio-App festlegen. In Schlüsselbundverwaltung den passenden Account öffnen, unter **Zugriffssteuerung** ausschließlich diese App hinzufügen/zulassen und die Änderung selbst mit dem Schlüsselbundpasswort bestätigen. Keine Freigabe für alle Programme erteilen. Danach in FrameStudio **Nur Schlüsselbund prüfen** wählen. Dieser Test liest aus dem OS-Speicher, auch wenn ein Sitzungsschlüssel aktiv ist; er sendet nichts an den Provider. Passwort und Schlüssel bleiben beim Nutzer. Eine Freigabe für einen ad-hoc signierten Build ist keine dauerhafte Lösung für zukünftige Builds.

## Dauerhafte Lösung für Updates

Builds mit derselben stabilen Code-Signing-Identität und passender Designated Requirement signieren; für reguläre Distribution Developer ID und Notarisierung. Auf diesem Mac ist eine entsprechende Identität noch nicht vorhanden. Das Anlegen/Importieren eines privaten Signierschlüssels und die erste Freigabe bestehender Einträge erfordern eine bewusste Einrichtung durch den Nutzer. Keine Zertifikate angelegt, keine Vertrauenseinstellungen oder Zugriffsrechte erweitert. Bis dahin bleibt der explizite Sitzungsschlüssel verfügbar.

Apple-Quellen, geprüft am 31.10.2026:
- [TN3127: Inside Code Signing: Requirements](https://developer.apple.com/documentation/technotes/tn3127-inside-code-signing-requirements)
- [Understanding the Code Signature](https://developer.apple.com/library/archive/documentation/Security/Conceptual/CodeSigningGuide/AboutCS/AboutCS.html)

Nachweise: `.artifacts/keychain-native-roundtrip.log`, `.artifacts/keychain-labeled-roundtrip.log`, `.artifacts/keychain-verified-write.log`. API-Schlüssel werden nach dauerhaftem Speichern direkt wieder gelesen und verglichen; erst dann wird Erfolg gemeldet. Scheitert die Kontrolle nach einem bestätigten Schreibvorgang, bleibt die mögliche Speicherung ausdrücklich benannt. Kein Klartext-Fallback, keine automatischen Passwortdialoge.
