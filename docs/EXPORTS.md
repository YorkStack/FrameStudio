# Exports and project packages

*Deutsch: [de/EXPORTS.md](de/EXPORTS.md)*

As of 2026-10-31. The installed application generates all formats in the Rust core. Python, Node, LibreOffice, PowerPoint or any other helper process is not required. LibreOffice and Poppler were used only for development acceptance.

## Formats and options

`export.create` produces original files, PNG, JPEG, WebP, TIFF, an offline HTML gallery, PDF, PPTX, a ZIP project package or a sprite sheet. The core offers the same function to GUI, REST and MCP. `export.submit` / `export.get` provide persistent asynchronous processing for larger exports.

- **Original:** unchanged bytes, including any source metadata already embedded. To strip it, export PNG/JPEG/WebP/TIFF with metadata export disabled. These conversions decode and re-encode the pixels; original EXIF/IPTC blocks are not copied.
- PNG, WebP and TIFF keep existing transparency. JPEG combines alpha channels with the explicitly chosen `background` color in `#RRGGBB` format (default white).
- `width`, `height`, `fit`: `preserve` keeps the ratio within the target bounds; `contain` fills the target size with a border; `cover` crops explicitly; `fill` stretches explicitly to the target size. Scaling never adds real detail.
- **HTML** contains local images and escaped text. A CSP prevents scripts and external content. For several files, `export.read` returns a portable ZIP.
- **PDF and PPTX** support `layout: single|contact`. Contact sheets contain four images per page/slide. `title`, `description`, `page_width_pt` and `page_height_pt` are configurable. PDF defaults to A4 landscape, PPTX to 16:9. Page sizes: 300–2000 pt. Titles up to 180 and the overall description up to 600 characters. Long individual captions are visibly shortened with `…`; `include_metadata: true` additionally attaches the full texts as JSON. Nothing is cut off silently.
- **PPTX** uses native text objects and embedded PNG images. Texts stay editable. The presentation is a complete OOXML package with master, layout, theme and relationships. Fonts may be substituted on the recipient's machine.
- **PDF** embeds Noto Sans and contains a Unicode character map; Latin text including German captions is tested. Characters not in the bundled font (e.g. many CJK characters) will need an additional font in the future.
- **Sprite sheets:** `cell_width`, `cell_height`, `columns`, `padding` (0–256) define grid and order. Every image is fitted proportionally and completely, never cropped automatically. `sprite.json` contains asset ID, order, cell coordinates and original dimensions.

The default naming scheme is `{projekt}_{beschreibung}_{nummer:04}`. Further placeholders: `{charakter}`, `{pose}`, `{ausdruck}`, `{datum}`, `{variante}` and `{nummer}` (the placeholder names are German: project, description, number, character, pose, expression, date, variant). Unknown placeholders are rejected. Invalid Windows characters, reserved device names and control characters are sanitized. The base name is limited to 100 UTF-8 bytes. Every export gets an exclusive UUID directory; repeated file names get a suffix. Concurrent jobs never overwrite files.

## Publishing and access

All files are first written and synced in a temporary directory inside `exports/`. Only a successful, complete export becomes visible through a rename. Failures leave no partially published output. The internal export can be retrieved via authenticated `export.read` as MIME type plus Base64; several files are delivered as ZIP. A deliberately configured additional export directory receives an atomic copy. The canonical internal export remains available for API/MCP. Target folders are never overwritten automatically.

At most 1000 assets per export; 64 MiB per input image; 32 megapixels per image, target image or sprite sheet; ZIP package and download at most 512 MiB in total. Split larger exports. Import limits and additional REST/MCP transport limits may be tighter. Decoding also has a 256 MiB allocation limit.

## Backup and restore

`format: zip, include_metadata: true` backs up projects, original assets, related styles, characters and historical jobs in a versioned `manifest.json`. Thumbnails are regenerated on import. Settings, provider keys and local access tokens are not part of the format. Domain fields are copied by allowlist; known secret fields are removed recursively. Free text remains user data: no export can reliably detect secrets deliberately typed into descriptions.

Without metadata, a shareable ZIP contains the project name and assets but no prompts, character definitions or job history. This ZIP can be imported too; it is not a complete backup of historical costs.

Restore checks version, size, entry count, compression ratio, duplicate names, missing or unassigned files, image signature, decoded dimensions, path traversal and symlinks. Absolute paths, `..`, Windows drive letters and backslashes are rejected. Only after successful validation are new files published under a random folder. The core assigns new IDs, writes the new project objects in a transaction and never turns archived requests into new paid jobs.

Originals are neither changed nor removed. After an interruption, orphaned staging or asset files may remain if the database transaction fails afterwards; they are not a published project. Presumably orphaned originals are never deleted automatically.

## Style previews in project packages

Metadata backups keep the editable preview subject. The link to the preview image is kept only if the related asset is part of the package. A preview from another project or from a selection that was not exported is not carried over as a broken reference. For complete style previews, back up the "Stilvorschauen" (style previews) project with original assets and metadata. Restore assigns new IDs and checks the connection to the historical style snapshot; it never starts a new generation.

## Acceptance

`cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --test exports` checks real decoding of all raster formats, transparency and JPEG background, byte-identical originals, Unicode PDF text and page count, PPTX text/images/relationships, safe offline HTML output, ZIP round trip, metadata exclusion, portable concurrent names, sprite geometry, invalid dimensions, unsafe paths, symlinks and malformed manifest types.

`cargo test --manifest-path src-tauri/Cargo.toml --no-default-features --test exports visual_proof -- --ignored` generates only synthetic test images plus PDF/PPTX under `.artifacts/export-proof/`. Both PPTX layouts were actually opened in LibreOffice and rendered to PDF. PDF and PPTX were rendered to PNG with Poppler and checked visually: German characters legible, no cut-off images or captions and undistorted aspect ratios in single and contact layouts. This is not an acceptance test in Microsoft PowerPoint.

## Dependencies and sources

- `image 0.25.10`: real image decoders, encoders and scaling instead of renaming file extensions. [Crate](https://crates.io/crates/image/0.25.10)
- `zip 9.2.0`, Deflate only: portable ZIP/OOXML packages without an external process. [Crate](https://crates.io/crates/zip/9.2.0)
- `lopdf 0.46.0`: PDF objects, streams and verifiable parsing; no additional Office runtime. [Crate](https://crates.io/crates/lopdf/0.46.0)
- `ttf-parser 0.25.1`: glyph metrics and Unicode mapping, only for the embedded trusted font. [Crate](https://crates.io/crates/ttf-parser/0.25.1). The audit reports **RUSTSEC-2026-0192: unmaintained**, with no known exploitable vulnerability. Temporary exception: no user fonts are processed; switch to the maintained `skrifa` before extending font support.
- [Noto Sans](https://github.com/notofonts/noto-fonts/tree/main/hinted/ttf/NotoSans), SIL OFL 1.1, shipped in full including `resources/OFL.txt`. SHA-256 of the embedded `NotoSans-Regular.ttf`: `b85c38ecea8a7cfb39c24e395a4007474fa5a4fc864f6ee33309eb4948d232d5`. Obtained and checked 2026-10-31. No font download at runtime.
