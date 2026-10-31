# Exporte und Projektpakete

Stand: 31.10.2026. Die installierte Anwendung erzeugt sämtliche Formate im Rust-Kern. Es gibt keine Voraussetzung für Python, Node, LibreOffice, PowerPoint oder einen anderen Hilfsprozess. LibreOffice und Poppler wurden ausschließlich in der Entwicklungsabnahme verwendet.

## Formate und Optionen

`export.create` erzeugt Originaldateien, PNG, JPEG, WebP, TIFF, eine offline nutzbare HTML-Galerie, PDF, PPTX, ein ZIP-Projektpaket oder ein Sprite-Sheet. Der Kern stellt dieselbe Funktion GUI, REST und MCP zur Verfügung. `export.submit` / `export.get` ermöglichen persistente asynchrone Verarbeitung größerer Exporte.

- Original: unveränderte Bytes einschließlich eventuell bereits eingebetteter Quellmetadaten. Wer diese entfernen möchte, exportiert PNG/JPEG/WebP/TIFF mit deaktiviertem Metadatenexport. Die Konvertierungen dekodieren und kodieren die Pixel neu; ursprüngliche EXIF/IPTC-Blöcke werden nicht kopiert.
- PNG, WebP und TIFF erhalten vorhandene Transparenz. JPEG kombiniert Alphakanäle mit der ausdrücklich gewählten Farbe `background` im Format `#RRGGBB` (Standard Weiß).
- `width`, `height`, `fit`: `preserve` erhält das Verhältnis innerhalb der Zielgrenzen; `contain` füllt die Zielgröße mit Rand; `cover` schneidet ausdrücklich zu; `fill` verzerrt ausdrücklich auf die Zielmaße. Skalieren erzeugt keine zusätzlichen echten Details.
- HTML enthält lokale Bilder und escaped Text. Eine CSP verhindert Skripte und externe Inhalte. Bei mehreren Dateien liefert `export.read` ein transportables ZIP.
- PDF und PPTX unterstützen `layout: single|contact`. Kontaktbögen enthalten vier Bilder pro Seite/Folie. `title`, `description`, `page_width_pt` und `page_height_pt` sind einstellbar. PDF standardmäßig A4 quer; PPTX standardmäßig 16:9. Seitenmaße: 300–2000 pt. Titel bis 180 und Gesamtbeschreibung bis 600 Zeichen. Lange Einzelbeschriftungen werden sichtbar mit `…` gekürzt; `include_metadata: true` legt vollständige Texte zusätzlich als JSON bei. Nicht unbemerkt abschneiden.
- PPTX verwendet native Textobjekte und eingebettete PNG-Bilder. Texte bleiben editierbar. Die Präsentation enthält ein vollständiges OOXML-Paket mit Master, Layout, Theme und Beziehungen. Schriften können beim Empfänger substituiert werden.
- PDF bettet Noto Sans ein und enthält eine Unicode-Zeichenzuordnung; lateinische einschließlich deutscher Beschriftungen sind getestet. Nicht im mitgelieferten Font enthaltene Zeichen (z. B. viele CJK-Zeichen) benötigen künftig einen zusätzlichen Font.
- Sprite-Sheets: `cell_width`, `cell_height`, `columns`, `padding` (0–256) legen Raster und Reihenfolge fest. Jedes Bild wird vollständig proportional eingepasst, niemals automatisch zugeschnitten. `sprite.json` enthält Asset-ID, Reihenfolge, Zellkoordinaten und ursprüngliche Maße.

Das Standardnamensschema lautet `{projekt}_{beschreibung}_{nummer:04}`. Weitere Platzhalter: `{charakter}`, `{pose}`, `{ausdruck}`, `{datum}`, `{variante}` und `{nummer}`. Unbekannte Platzhalter werden abgelehnt. Ungültige Windows-Zeichen, reservierte Gerätenamen und Steuerzeichen werden bereinigt. Der Basisname wird auf höchstens 100 UTF-8-Bytes begrenzt. Jeder Export erhält ein exklusives UUID-Verzeichnis; wiederholte Dateinamen bekommen einen Suffix. Gleichzeitige Aufträge überschreiben keine Dateien.

## Veröffentlichung und Zugriff

Zunächst werden alle Dateien in einem temporären Verzeichnis innerhalb `exports/` geschrieben und synchronisiert. Erst ein erfolgreicher vollständiger Export wird durch Umbenennen sichtbar. Fehler hinterlassen keine teilweise veröffentlichte Ausgabe. Der interne Export ist mit `export.read` authentifiziert als MIME-Typ plus Base64 abrufbar; mehrere Dateien werden als ZIP geliefert. Ein bewusst konfiguriertes zusätzliches Exportverzeichnis erhält eine atomare Kopie. Der kanonische interne Export bleibt für API/MCP verfügbar. Die Zielordner werden niemals automatisch überschrieben.

Maximal 1000 Assets pro Export; 64 MiB pro Eingangsbild; 32 Megapixel pro Bild/Zielbild/Sprite-Sheet; ZIP-Paket und Download insgesamt maximal 512 MiB. Größere Exporte müssen aufgeteilt werden. Die Importgrenzen und zusätzliche REST-/MCP-Transportgrenzen können enger sein. Decodierung erhält zusätzlich ein 256-MiB-Allokationslimit.

## Backup und Wiederherstellung

`format: zip, include_metadata: true` sichert Projekte, Originalassets, zugehörige Stile, Charaktere und historische Jobs in einem versionierten `manifest.json`. Thumbnails werden beim Import neu erzeugt. Einstellungen, Provider-Schlüssel und lokale Zugriffstokens sind kein Bestandteil des Formats. Domain-Felder werden per Positivliste übernommen; bekannte Geheimnisfelder werden rekursiv entfernt. Freitext bleibt Benutzerdatum: bewusst in Beschreibungen eingegebene Geheimnisse kann kein Export zuverlässig erkennen.

Ohne Metadaten enthält ein teilbares ZIP Projektbezeichnung und Assets, aber keine Prompts, Charakterdefinitionen oder Jobhistorie. Auch dieses ZIP ist importierbar; es ist keine vollständige Sicherung der historischen Kosten.

Die Wiederherstellung prüft Version, Größe, Eintragszahl, Kompressionsverhältnis, doppelte Namen, fehlende oder nicht zugeordnete Dateien, Bildsignatur, dekodierte Maße, Pfadtraversal und Symlinks. Absolute Pfade, `..`, Windows-Laufwerke und Backslashes werden abgelehnt. Erst nach erfolgreicher Validierung werden neue Dateien unter einem zufälligen Ordner veröffentlicht. Der Kern vergibt neue IDs, schreibt die neuen Projektobjekte transaktional und nimmt archivierte Requests niemals als neue kostenpflichtige Jobs wieder auf.

Originale werden weder verändert noch entfernt. Nach einem Abbruch können verwaiste Staging-/Assetdateien bestehen bleiben, wenn die Datenbanktransaktion anschließend scheitert; sie sind kein veröffentlichtes Projekt. Es gibt keine automatische Löschung mutmaßlich verwaister Originale.

## Abnahme

`cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --test exports` prüft echte Dekodierung aller Rasterformate, Transparenz/JPEG-Hintergrund, byteidentische Originale, Unicode-PDF-Text und Seitenanzahl, PPTX-Text/Bilder/Beziehungen, sichere Offline-HTML-Ausgabe, ZIP-Roundtrip, Metadatenausschluss, portable parallele Namen, Sprite-Geometrie, ungültige Maße, unsichere Pfade, Symlinks und fehlerhafte Manifesttypen.

`cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --test exports visual_proof -- --ignored` erzeugt ausschließlich synthetische Prüfbilder und PDF/PPTX unter `.artifacts/export-proof/`. Beide PPTX-Layouts wurden mit LibreOffice tatsächlich geöffnet und nach PDF gerendert. PDF und PPTX wurden mit Poppler als PNG gerendert und visuell kontrolliert: deutsche Zeichen lesbar, keine abgeschnittenen Bilder oder Beschriftungen und unverzerrte Seitenverhältnisse in Einzel- und Kontaktlayout. Dies ist keine Abnahme in Microsoft PowerPoint.

## Abhängigkeiten und Quellen

- `image 0.25.10`: echte Bilddecoder, Encoder und Skalierung; statt Dateiendungswechsel. [Crate](https://crates.io/crates/image/0.25.10)
- `zip 9.2.0`, nur Deflate: portable ZIP-/OOXML-Pakete ohne externen Prozess. [Crate](https://crates.io/crates/zip/9.2.0)
- `lopdf 0.46.0`: PDF-Objekte, Streams und überprüfbares Parsing; keine zusätzliche Office-Runtime. [Crate](https://crates.io/crates/lopdf/0.46.0)
- `ttf-parser 0.25.1`: Glyphenmetriken und Unicode-Zuordnung ausschließlich für den eingebetteten vertrauenswürdigen Font. [Crate](https://crates.io/crates/ttf-parser/0.25.1). Audit meldet **RUSTSEC-2026-0192: unmaintained**, keine bekannte ausnutzbare Schwachstelle. Befristete Ausnahme für 0.1: keine Nutzerfonts werden verarbeitet; Umstellung auf gepflegtes `skrifa` vor Erweiterung der Schriftunterstützung und nächstem Wartungsrelease.
- [Noto Sans](https://github.com/notofonts/noto-fonts/tree/main/hinted/ttf/NotoSans), SIL OFL 1.1, vollständig mitgeliefert einschließlich `resources/OFL.txt`. SHA-256 der eingebetteten `NotoSans-Regular.ttf`: `b85c38ecea8a7cfb39c24e395a4007474fa5a4fc864f6ee33309eb4948d232d5`. Bezug und Prüfung 31.10.2026. Kein Font-Download zur Laufzeit.

## Stilvorschauen in Projektpaketen

Metadaten-Backups erhalten das editierbare Vorschaumotiv. Die Vorschaubild-Verknüpfung bleibt nur erhalten, wenn das zugehörige Asset im Paket enthalten ist. Eine Vorschau aus einem anderen Projekt oder einer nicht exportierten Auswahl wird nicht als kaputter Verweis übernommen. Für vollständige Stilvorschauen das Projekt „Stilvorschauen“ mit Originalassets und Metadaten sichern. Wiederherstellung vergibt neue IDs und prüft die Verbindung zum historischen Stilsnapshot; sie startet keine neue Generierung.
