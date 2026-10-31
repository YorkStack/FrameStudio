//! Portable, bounded exports. Originals are read-only; output is published atomically.
use anyhow::{Context, Result, anyhow, bail, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use image::{DynamicImage, ImageFormat, ImageReader, Rgba, RgbaImage, imageops};
use lopdf::{Document, Object, Stream, dictionary};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;
use zip::write::SimpleFileOptions;

const MAX_FILE: u64 = 64 * 1024 * 1024;
const MAX_ARCHIVE: u64 = 512 * 1024 * 1024;
const MAX_PIXELS: u64 = 32_000_000;
const FONT: &[u8] = include_bytes!("../resources/NotoSans-Regular.ttf");

fn string<'a>(value: &'a Value, key: &str, default: &'a str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or(default)
}

fn dimension(options: &Value, key: &str, default: u32) -> Result<u32> {
    let n = options
        .get(key)
        .filter(|v| !v.is_null())
        .map_or(Ok(default as u64), |v| {
            v.as_u64().ok_or_else(|| anyhow!("Ungültiges Maß: {key}"))
        })?;
    ensure!(
        (1..=16384).contains(&n),
        "Maß {key} muss zwischen 1 und 16384 liegen"
    );
    Ok(n as u32)
}

/// All caller supplied paths stay beneath the application root and reject symlinks.
pub fn safe_file(root: &Path, relative: &str) -> Result<PathBuf> {
    let rel = Path::new(relative);
    ensure!(
        !relative.contains('\\') && !relative.contains(':') && !rel.is_absolute(),
        "Ungültiger Dateipfad"
    );
    let mut path = root.to_path_buf();
    for component in rel.components() {
        let Component::Normal(part) = component else {
            bail!("Ungültiger Dateipfad")
        };
        path.push(part);
        ensure!(
            !fs::symlink_metadata(&path)?.file_type().is_symlink(),
            "Symbolische Links sind nicht erlaubt"
        );
    }
    ensure!(path.is_file(), "Datei fehlt");
    ensure!(
        path.canonicalize()?.starts_with(root.canonicalize()?),
        "Datei liegt außerhalb des Speichers"
    );
    Ok(path)
}

fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>> {
    ensure!(
        fs::metadata(path)?.len() <= limit,
        "Datei überschreitet die Größenbegrenzung"
    );
    let mut bytes = Vec::new();
    File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= limit,
        "Datei überschreitet die Größenbegrenzung"
    );
    Ok(bytes)
}

pub fn decode_image(bytes: &[u8]) -> Result<DynamicImage> {
    ensure!(
        bytes.len() as u64 <= MAX_FILE,
        "Bilddatei ist zu groß (maximal 64 MiB)"
    );
    let format = image::guess_format(bytes).context("Datei enthält kein unterstütztes Bild")?;
    ensure!(
        matches!(
            format,
            ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP | ImageFormat::Tiff
        ),
        "Nur PNG, JPEG, WebP und TIFF werden unterstützt"
    );
    let (w, h) = ImageReader::with_format(Cursor::new(bytes), format).into_dimensions()?;
    ensure!(
        w > 0 && h > 0 && u64::from(w) * u64::from(h) <= MAX_PIXELS,
        "Bild überschreitet 32 Megapixel"
    );
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(256 * 1024 * 1024);
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    reader.limits(limits);
    Ok(reader.decode()?)
}

fn atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let temp = path.with_extension(format!("{}.part", Uuid::new_v4()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        ensure!(!path.exists(), "Exportdatei existiert bereits");
        fs::rename(&temp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

pub fn safe_name(input: &str) -> String {
    let mut name: String = input
        .chars()
        .take(80)
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    while name.len() > 100 {
        name.pop();
    }
    name = name.trim().trim_matches('.').to_string();
    if name.is_empty() {
        name = "Grafik".into();
    }
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if reserved.contains(&stem.as_str()) {
        name.insert(0, '_');
    }
    name
}

pub fn filename(project: &Value, asset: &Value, options: &Value, number: usize) -> Result<String> {
    let template = string(
        options,
        "naming_template",
        "{projekt}_{beschreibung}_{nummer:04}",
    );
    ensure!(template.len() <= 500, "Namensschema ist zu lang");
    let mut validated = template.to_string();
    for token in [
        "{projekt}",
        "{beschreibung}",
        "{nummer:04}",
        "{nummer}",
        "{charakter}",
        "{pose}",
        "{ausdruck}",
        "{datum}",
        "{variante}",
    ] {
        validated = validated.replace(token, "");
    }
    ensure!(
        !validated.contains('{') && !validated.contains('}'),
        "Unbekannter Platzhalter im Namensschema"
    );
    let mut name = template
        .replace("{projekt}", &safe_name(string(project, "name", "Projekt")))
        .replace(
            "{beschreibung}",
            &safe_name(string(asset, "description", "Grafik")),
        )
        .replace("{nummer:04}", &format!("{number:04}"))
        .replace("{nummer}", &number.to_string());
    for key in ["charakter", "pose", "ausdruck", "datum", "variante"] {
        let field = match key {
            "charakter" => "character_name",
            "ausdruck" => "expression",
            "datum" => "created_at",
            "variante" => "variant",
            _ => key,
        };
        let fallback = match key {
            "charakter" => string(&asset["snapshot"]["character"], "name", ""),
            "pose" | "ausdruck" => string(&asset["snapshot"]["blocks"], field, ""),
            _ => "",
        };
        let value = string(asset, field, fallback);
        let value = if key == "datum" {
            value.get(..10).unwrap_or(value)
        } else {
            value
        };
        name = name.replace(&format!("{{{key}}}"), value);
    }
    Ok(safe_name(&name))
}

fn bg(options: &Value) -> Result<Rgba<u8>> {
    let value = string(options, "background", "#ffffff");
    ensure!(
        value.len() == 7
            && value.starts_with('#')
            && value[1..].bytes().all(|b| b.is_ascii_hexdigit()),
        "Hintergrundfarbe muss #RRGGBB sein"
    );
    Ok(Rgba([
        u8::from_str_radix(&value[1..3], 16)?,
        u8::from_str_radix(&value[3..5], 16)?,
        u8::from_str_radix(&value[5..7], 16)?,
        255,
    ]))
}

fn resized(image: DynamicImage, options: &Value) -> Result<DynamicImage> {
    if options.get("width").is_none_or(Value::is_null)
        && options.get("height").is_none_or(Value::is_null)
    {
        return Ok(image);
    }
    let width = dimension(options, "width", image.width())?;
    let height = dimension(options, "height", image.height())?;
    ensure!(
        u64::from(width) * u64::from(height) <= MAX_PIXELS,
        "Zielbild überschreitet 32 Megapixel"
    );
    let filter = imageops::FilterType::Lanczos3;
    match string(options, "fit", "contain") {
        "preserve" => Ok(image.resize(width, height, filter)),
        "cover" => Ok(image.resize_to_fill(width, height, filter)),
        "fill" => Ok(image.resize_exact(width, height, filter)),
        "contain" => {
            let small = image.resize(width, height, filter);
            let mut canvas = RgbaImage::from_pixel(
                width,
                height,
                if image.color().has_alpha() {
                    Rgba([0, 0, 0, 0])
                } else {
                    bg(options)?
                },
            );
            imageops::overlay(
                &mut canvas,
                &small,
                i64::from((width - small.width()) / 2),
                i64::from((height - small.height()) / 2),
            );
            Ok(DynamicImage::ImageRgba8(canvas))
        }
        _ => bail!("Unbekannter Skalierungsmodus"),
    }
}

fn flatten(image: &DynamicImage, background: Rgba<u8>) -> DynamicImage {
    let mut canvas = RgbaImage::from_pixel(image.width(), image.height(), background);
    imageops::overlay(&mut canvas, image, 0, 0);
    DynamicImage::ImageRgb8(DynamicImage::ImageRgba8(canvas).to_rgb8())
}

fn encode(image: &DynamicImage, format: ImageFormat, options: &Value) -> Result<Vec<u8>> {
    let mut output = Cursor::new(Vec::new());
    if format == ImageFormat::Jpeg {
        flatten(image, bg(options)?).write_to(&mut output, format)?;
    } else {
        image.write_to(&mut output, format)?;
    }
    Ok(output.into_inner())
}

fn curated(value: &Value, fields: &[&str]) -> Value {
    let mut result = Map::new();
    for key in fields {
        if let Some(v) = value.get(*key) {
            result.insert((*key).into(), redact(v));
        }
    }
    Value::Object(result)
}

fn redact(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(k, _)| {
                    let key = k.to_ascii_lowercase();
                    ![
                        "key",
                        "api_key",
                        "apikey",
                        "secret",
                        "token",
                        "access_token",
                        "refresh_token",
                        "authorization",
                        "password",
                        "credential",
                        "credentials",
                    ]
                    .contains(&key.as_str())
                })
                .map(|(k, v)| (k.clone(), redact(v)))
                .collect(),
        ),
        Value::Array(list) => Value::Array(list.iter().map(redact).collect()),
        _ => value.clone(),
    }
}

fn domain_record(value: &Value, kind: &str) -> Value {
    let fields: &[&str] = match kind {
        "style" => &[
            "id",
            "name",
            "blocks",
            "preview_prompt",
            "preview_asset_id",
            "preview_style_version",
            "preview_provider",
            "preview_hidden",
            "version",
            "versions",
            "created_at",
            "updated_at",
        ],
        "character" => &[
            "id",
            "project_id",
            "name",
            "description",
            "features",
            "clothing",
            "colors",
            "binding",
            "reference_ids",
            "approved",
            "portrait_asset_id",
            "created_at",
            "updated_at",
        ],
        "job" => &[
            "id",
            "kind",
            "result_text",
            "result_sections",
            "project_id",
            "status",
            "created_at",
            "finished_at",
            "idempotency_key",
            "request_fingerprint",
            "batch_count",
            "snapshot",
            "estimated_microusd",
            "reserved_microusd",
            "actual_microusd",
            "cost_status",
            "error",
            "cancel_requested",
            "provider_request_id",
            "usage",
            "asset_ids",
        ],
        _ => &[],
    };
    let mut result = curated(value, fields);
    if let Some(snapshot) = result.get_mut("snapshot") {
        *snapshot = snapshot_metadata(snapshot);
    }
    if let Some(blocks) = result.get_mut("blocks") {
        *blocks = curated(
            blocks,
            &[
                "medium",
                "lines",
                "palette",
                "light",
                "texture",
                "goal",
                "scene",
                "pose",
                "expression",
                "gaze",
                "mouth",
                "composition",
                "output",
                "avoid",
                "view",
            ],
        );
    }
    if let Some(versions) = result.get_mut("versions") {
        *versions = json!(
            versions
                .as_array()
                .map(|items| items
                    .iter()
                    .map(|v| domain_record(v, "style"))
                    .collect::<Vec<_>>())
                .unwrap_or_default()
        );
    }
    result
}

fn domain_list(value: &Value, kind: &str) -> Value {
    json!(
        value
            .as_array()
            .map(|items| items
                .iter()
                .map(|v| domain_record(v, kind))
                .collect::<Vec<_>>())
            .unwrap_or_default()
    )
}

fn snapshot_metadata(value: &Value) -> Value {
    let mut snapshot = curated(
        value,
        &[
            "version",
            "kind",
            "project_id",
            "system",
            "created_at",
            "project",
            "style",
            "character",
            "blocks",
            "references",
            "prompt",
            "parameters",
            "price_snapshot",
        ],
    );
    if let Some(style) = snapshot.get_mut("style") {
        *style = domain_record(style, "style");
    }
    if let Some(character) = snapshot.get_mut("character") {
        *character = domain_record(character, "character");
    }
    if let Some(project) = snapshot.get_mut("project") {
        *project = curated(
            project,
            &[
                "id",
                "name",
                "description",
                "created_at",
                "aspect_ratio",
                "style_id",
                "budget_microusd",
                "archived",
                "prompt_template",
            ],
        );
    }
    if let Some(parameters) = snapshot.get_mut("parameters") {
        *parameters = curated(
            parameters,
            &[
                "project_id",
                "description",
                "style_id",
                "character_id",
                "reference_ids",
                "parent_id",
                "provider",
                "model",
                "aspect_ratio",
                "resolution",
                "quality",
                "count",
                "blocks",
                "unknown_price_approved",
                "text",
                "instruction",
                "max_output_tokens",
                "target",
            ],
        );
    }
    if let Some(references) = snapshot.get_mut("references") {
        *references = json!(
            references
                .as_array()
                .map(|items| items
                    .iter()
                    .map(|r| curated(
                        r,
                        &["id", "path", "sha256", "mime", "width", "height", "format"]
                    ))
                    .collect::<Vec<_>>())
                .unwrap_or_default()
        );
    }
    snapshot
}

fn asset_metadata(asset: &Value, include: bool) -> Value {
    let fields = if include {
        vec![
            "id",
            "project_id",
            "description",
            "created_at",
            "provider",
            "model",
            "parameters",
            "prompt",
            "snapshot",
            "reference_ids",
            "parent_id",
            "character_id",
            "width",
            "height",
            "format",
            "mime",
            "sha256",
            "cost_status",
            "cost_microusd",
            "job_id",
            "tags",
            "favorite",
            "comment",
            "status",
            "pose",
            "expression",
            "variant",
            "character_name",
        ]
    } else {
        vec![
            "id",
            "description",
            "width",
            "height",
            "format",
            "created_at",
        ]
    };
    let mut value = curated(asset, &fields);
    if let Some(snapshot) = value.get_mut("snapshot") {
        *snapshot = snapshot_metadata(snapshot);
    }
    value
}

fn escape(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '&' => "&amp;".into(),
            '<' => "&lt;".into(),
            '>' => "&gt;".into(),
            '"' => "&quot;".into(),
            '\'' => "&#39;".into(),
            _ if c.is_control() && c != '\n' && c != '\t' => String::new(),
            _ => c.to_string(),
        })
        .collect()
}

fn source(root: &Path, asset: &Value) -> Result<(Vec<u8>, DynamicImage)> {
    let path = safe_file(root, string(asset, "path", ""))?;
    let bytes = read_bounded(&path, MAX_FILE)?;
    let image = decode_image(&bytes)?;
    Ok((bytes, image))
}

fn zip_file(zip: &mut zip::ZipWriter<Cursor<Vec<u8>>>, name: &str, bytes: &[u8]) -> Result<()> {
    zip.start_file(
        name,
        SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o600),
    )?;
    zip.write_all(bytes)?;
    Ok(())
}

fn image_exports(
    root: &Path,
    dir: &Path,
    project: &Value,
    assets: &[Value],
    options: &Value,
) -> Result<Vec<String>> {
    let format = string(options, "format", "png");
    let mut names = HashSet::new();
    let mut files = Vec::new();
    for (index, asset) in assets.iter().enumerate() {
        let (bytes, img) = source(root, asset)?;
        let ext = if format == "original" {
            image::guess_format(&bytes)?.extensions_str()[0]
        } else {
            format
        };
        let mut name = format!("{}.{}", filename(project, asset, options, index + 1)?, ext);
        let mut suffix = index + 1;
        while !names.insert(name.clone()) {
            name = format!(
                "{}_{}.{}",
                filename(project, asset, options, index + 1)?,
                suffix,
                ext
            );
            suffix += 1;
        }
        let data = if format == "original" {
            bytes
        } else {
            let fmt = match format {
                "png" => ImageFormat::Png,
                "jpeg" => ImageFormat::Jpeg,
                "webp" => ImageFormat::WebP,
                "tiff" => ImageFormat::Tiff,
                _ => bail!("Unbekanntes Bildformat"),
            };
            encode(&resized(img, options)?, fmt, options)?
        };
        atomic(&dir.join(&name), &data)?;
        files.push(name);
    }
    Ok(files)
}

fn html_export(
    root: &Path,
    dir: &Path,
    project: &Value,
    assets: &[Value],
    options: &Value,
) -> Result<Vec<String>> {
    let mut files = Vec::new();
    let title = string(options, "title", string(project, "name", "Galerie"));
    let mut html = format!(
        "<!doctype html><html lang=\"de\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; img-src 'self'; style-src 'unsafe-inline'\"><title>{}</title><style>body{{font:16px system-ui;background:#f7f5ef;color:#282723;margin:40px}}main{{display:grid;grid-template-columns:repeat(auto-fit,minmax(260px,1fr));gap:24px}}figure{{margin:0;background:white;padding:16px;break-inside:avoid}}img{{width:100%;height:300px;object-fit:contain}}figcaption{{overflow-wrap:anywhere;white-space:pre-wrap;line-height:1.5}}h1{{font-size:36px;overflow-wrap:anywhere}}p{{white-space:pre-wrap;overflow-wrap:anywhere}}</style><h1>{}</h1><p>{}</p><main>",
        escape(title),
        escape(title),
        escape(string(options, "description", ""))
    );
    for (i, asset) in assets.iter().enumerate() {
        let (_, img) = source(root, asset)?;
        let name = format!("image-{:04}.png", i + 1);
        atomic(
            &dir.join(&name),
            &encode(&resized(img, options)?, ImageFormat::Png, options)?,
        )?;
        html.push_str(&format!("<figure><a href=\"{name}\"><img loading=\"lazy\" src=\"{name}\" alt=\"{}\"></a><figcaption>{}</figcaption></figure>",escape(string(asset,"description","Grafik")),escape(string(asset,"description","Grafik"))));
        files.push(name);
    }
    html.push_str("</main></html>");
    atomic(&dir.join("galerie.html"), html.as_bytes())?;
    files.insert(0, "galerie.html".into());
    Ok(files)
}

fn backup(root: &Path, project: &Value, assets: &[Value], options: &Value) -> Result<Vec<u8>> {
    let include = options["include_metadata"].as_bool().unwrap_or(false);
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let mut records = Vec::new();
    let mut total = 0u64;
    for (i, asset) in assets.iter().enumerate() {
        let (bytes, _) = source(root, asset)?;
        total += bytes.len() as u64;
        ensure!(
            total <= MAX_ARCHIVE,
            "Projekt überschreitet die Paketgrenze von 512 MiB"
        );
        let ext = image::guess_format(&bytes)?.extensions_str()[0];
        let path = format!("assets/{:04}.{ext}", i + 1);
        zip_file(&mut zip, &path, &bytes)?;
        let mut entry = asset_metadata(asset, include);
        entry["path"] = json!(path);
        records.push(entry);
    }
    let mut p = curated(
        project,
        &["id", "name", "description", "created_at", "aspect_ratio"],
    );
    if include {
        p = curated(
            project,
            &[
                "id",
                "name",
                "description",
                "created_at",
                "aspect_ratio",
                "style_id",
                "preferences",
                "budget_microusd",
                "archived",
                "prompt_template",
            ],
        );
    }
    let manifest = json!({"version":1,"application":"FrameStudio","metadata_included":include,"project":p,"assets":records,"styles":if include {domain_list(&project["backup_styles"],"style")}else{json!([])},"characters":if include {domain_list(&project["backup_characters"],"character")}else{json!([])},"jobs":if include {domain_list(&project["backup_jobs"],"job")}else{json!([])}});
    zip_file(
        &mut zip,
        "manifest.json",
        &serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(zip.finish()?.into_inner())
}

fn sprite(root: &Path, dir: &Path, assets: &[Value], options: &Value) -> Result<Vec<String>> {
    let cw = dimension(options, "cell_width", 512)?;
    let ch = dimension(options, "cell_height", 512)?;
    let cols = dimension(options, "columns", 4)?.min(assets.len() as u32);
    let padding = options["padding"].as_u64().unwrap_or(8);
    ensure!(padding <= 256, "Padding darf maximal 256 Pixel betragen");
    let p = padding as u32;
    let rows = (assets.len() as u32).div_ceil(cols);
    let w = cols
        .checked_mul(cw + p)
        .and_then(|v| v.checked_add(p))
        .context("Sprite-Breite zu groß")?;
    let h = rows
        .checked_mul(ch + p)
        .and_then(|v| v.checked_add(p))
        .context("Sprite-Höhe zu groß")?;
    ensure!(
        u64::from(w) * u64::from(h) <= MAX_PIXELS,
        "Sprite-Sheet überschreitet 32 Megapixel"
    );
    let mut canvas = RgbaImage::new(w, h);
    let mut frames = Vec::new();
    for (i, asset) in assets.iter().enumerate() {
        let (_, img) = source(root, asset)?;
        let small = img.resize(cw, ch, imageops::FilterType::Lanczos3);
        let x = p + (i as u32 % cols) * (cw + p);
        let y = p + (i as u32 / cols) * (ch + p);
        imageops::overlay(
            &mut canvas,
            &small,
            i64::from(x + (cw - small.width()) / 2),
            i64::from(y + (ch - small.height()) / 2),
        );
        frames.push(json!({"asset_id":asset["id"],"index":i,"x":x,"y":y,"width":cw,"height":ch,"source_width":img.width(),"source_height":img.height(),"fit":"contain","trimmed":false}));
    }
    atomic(
        &dir.join("sprite.png"),
        &encode(&DynamicImage::ImageRgba8(canvas), ImageFormat::Png, options)?,
    )?;
    atomic(
        &dir.join("sprite.json"),
        &serde_json::to_vec_pretty(
            &json!({"version":1,"image":"sprite.png","width":w,"height":h,"padding":p,"frames":frames}),
        )?,
    )?;
    Ok(vec!["sprite.png".into(), "sprite.json".into()])
}

fn font_face() -> Result<ttf_parser::Face<'static>> {
    ttf_parser::Face::parse(FONT, 0).map_err(|_| anyhow!("Export-Schrift ist ungültig"))
}

fn text_width(face: &ttf_parser::Face<'_>, text: &str, size: f32) -> f32 {
    text.chars()
        .map(|c| {
            face.glyph_index(c)
                .and_then(|g| face.glyph_hor_advance(g))
                .unwrap_or(600) as f32
        })
        .sum::<f32>()
        * size
        / f32::from(face.units_per_em())
}

fn wrap(text: &str, width: f32, size: f32) -> Result<Vec<String>> {
    let face = font_face()?;
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for c in paragraph.chars().filter(|c| !c.is_control()) {
            if !line.is_empty() && text_width(&face, &format!("{line}{c}"), size) > width {
                lines.push(line.trim_end().to_string());
                line = String::new();
            }
            if !line.is_empty() || !c.is_whitespace() {
                line.push(c);
            }
        }
        lines.push(line);
    }
    Ok(lines)
}

fn preview_lines(text: &str, width: f32, size: f32, max: usize) -> Result<Vec<String>> {
    let mut lines = wrap(text, width, size)?;
    if lines.len() > max {
        lines.truncate(max);
        if let Some(last) = lines.last_mut() {
            let face = font_face()?;
            while !last.is_empty() && text_width(&face, &format!("{last}…"), size) > width {
                last.pop();
            }
            last.push('…');
        }
    }
    Ok(lines)
}

#[derive(Clone, Copy)]
struct PageLayout {
    width: f32,
    height: f32,
    columns: usize,
    rows: usize,
}

impl PageLayout {
    fn new(options: &Value, pptx: bool) -> Result<Self> {
        let width = options["page_width_pt"]
            .as_f64()
            .unwrap_or(if pptx { 960.0 } else { 841.89 }) as f32;
        let height = options["page_height_pt"]
            .as_f64()
            .unwrap_or(if pptx { 540.0 } else { 595.28 }) as f32;
        ensure!(
            width.is_finite()
                && height.is_finite()
                && (300.0..=2000.0).contains(&width)
                && (300.0..=2000.0).contains(&height),
            "Seitenmaße müssen 300–2000 pt betragen"
        );
        let contact = string(options, "layout", "single") == "contact";
        Ok(Self {
            width,
            height,
            columns: if contact { 2 } else { 1 },
            rows: if contact { 2 } else { 1 },
        })
    }
    fn per_page(self) -> usize {
        self.columns * self.rows
    }
    fn cell(self, index: usize) -> (f32, f32, f32, f32) {
        let gap = 18.0;
        let margin = 28.0;
        let width =
            (self.width - margin * 2.0 - gap * (self.columns - 1) as f32) / self.columns as f32;
        let height = (self.height - 130.0 - gap * (self.rows - 1) as f32) / self.rows as f32;
        (
            margin + (index % self.columns) as f32 * (width + gap),
            100.0 + (index / self.columns) as f32 * (height + gap),
            width,
            height,
        )
    }
}

fn pdf_font(doc: &mut Document, texts: &[String]) -> Result<lopdf::ObjectId> {
    let face = font_face()?;
    let mut mapping = BTreeMap::new();
    for ch in texts.iter().flat_map(|t| t.chars()) {
        if let Some(g) = face.glyph_index(ch) {
            mapping.insert(g.0, ch);
        }
    }
    let units = f32::from(face.units_per_em());
    let file = doc.add_object(Stream::new(
        dictionary! {"Length1"=>FONT.len() as i64},
        FONT.to_vec(),
    ));
    let bounds = face.global_bounding_box();
    let bbox: Vec<Object> = [bounds.x_min, bounds.y_min, bounds.x_max, bounds.y_max]
        .iter()
        .map(|x| Object::Real(*x as f32 * 1000.0 / units))
        .collect();
    let descriptor=doc.add_object(dictionary!{"Type"=>"FontDescriptor","FontName"=>"NotoSans","Flags"=>32,"FontBBox"=>bbox,"ItalicAngle"=>0,"Ascent"=>face.ascender() as i64,"Descent"=>face.descender() as i64,"CapHeight"=>714,"StemV"=>80,"FontFile2"=>file});
    let mut widths = Vec::new();
    for glyph in mapping.keys() {
        widths.push(Object::Integer(i64::from(*glyph)));
        let width = face
            .glyph_hor_advance(ttf_parser::GlyphId(*glyph))
            .unwrap_or(600) as f32
            * 1000.0
            / units;
        widths.push(Object::Array(vec![Object::Real(width)]));
    }
    let descendant=doc.add_object(dictionary!{"Type"=>"Font","Subtype"=>"CIDFontType2","BaseFont"=>"NotoSans","CIDSystemInfo"=>dictionary!{"Registry"=>Object::string_literal("Adobe"),"Ordering"=>Object::string_literal("Identity"),"Supplement"=>0},"FontDescriptor"=>descriptor,"DW"=>600,"W"=>widths,"CIDToGIDMap"=>"Identity"});
    let mut cmap = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    let pairs: Vec<_> = mapping.iter().collect();
    for chunk in pairs.chunks(100) {
        cmap.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for (glyph, ch) in chunk {
            let utf16: String = ch
                .encode_utf16(&mut [0; 2])
                .iter()
                .map(|c| format!("{c:04X}"))
                .collect();
            cmap.push_str(&format!("<{glyph:04X}> <{utf16}>\n"));
        }
        cmap.push_str("endbfchar\n");
    }
    cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend");
    let unicode = doc.add_object(Stream::new(dictionary! {}, cmap.into_bytes()));
    Ok(doc.add_object(dictionary!{"Type"=>"Font","Subtype"=>"Type0","BaseFont"=>"NotoSans","Encoding"=>"Identity-H","DescendantFonts"=>vec![Object::Reference(descendant)],"ToUnicode"=>unicode}))
}

fn pdf_text(
    ops: &mut Vec<lopdf::content::Operation>,
    text: &str,
    x: f32,
    y: f32,
    size: f32,
) -> Result<()> {
    use lopdf::content::Operation;
    let face = font_face()?;
    let bytes: Vec<u8> = text
        .chars()
        .flat_map(|c| {
            face.glyph_index(c)
                .unwrap_or(ttf_parser::GlyphId(0))
                .0
                .to_be_bytes()
        })
        .collect();
    ops.push(Operation::new("BT", vec![]));
    ops.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F1".to_vec()), size.into()],
    ));
    ops.push(Operation::new("Td", vec![x.into(), y.into()]));
    ops.push(Operation::new(
        "Tj",
        vec![Object::String(bytes, lopdf::StringFormat::Hexadecimal)],
    ));
    ops.push(Operation::new("ET", vec![]));
    Ok(())
}

fn fit_rect(w: u32, h: u32, x: f32, y: f32, width: f32, height: f32) -> (f32, f32, f32, f32) {
    let scale = (width / w as f32).min(height / h as f32);
    let iw = w as f32 * scale;
    let ih = h as f32 * scale;
    (x + (width - iw) / 2.0, y + (height - ih) / 2.0, iw, ih)
}

fn pdf_export(root: &Path, project: &Value, assets: &[Value], options: &Value) -> Result<Vec<u8>> {
    use lopdf::content::{Content, Operation};
    let layout = PageLayout::new(options, false)?;
    let title = string(options, "title", string(project, "name", "FrameStudio"));
    let description = string(options, "description", "");
    ensure!(
        title.chars().count() <= 180 && description.chars().count() <= 600,
        "Bitte Titel auf 180 und Beschreibung auf 600 Zeichen begrenzen"
    );
    let texts: Vec<String> = assets
        .iter()
        .map(|a| string(a, "description", "").to_string())
        .chain([
            title.into(),
            description.into(),
            "0123456789 /…FrameStudio".into(),
        ])
        .collect();
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font = pdf_font(&mut doc, &texts)?;
    let mut pages = Vec::new();
    let page_count = assets.len().div_ceil(layout.per_page());
    for (page_index, chunk) in assets.chunks(layout.per_page()).enumerate() {
        let mut ops = Vec::new();
        for (line_index, line) in preview_lines(title, layout.width - 56.0, 21.0, 2)?
            .iter()
            .enumerate()
        {
            pdf_text(
                &mut ops,
                line,
                28.0,
                layout.height - 35.0 - line_index as f32 * 25.0,
                21.0,
            )?;
        }
        for (line_index, line) in preview_lines(description, layout.width - 56.0, 10.0, 2)?
            .iter()
            .enumerate()
        {
            pdf_text(
                &mut ops,
                line,
                28.0,
                layout.height - 75.0 - line_index as f32 * 13.0,
                10.0,
            )?;
        }
        let mut images = lopdf::Dictionary::new();
        for (index, asset) in chunk.iter().enumerate() {
            let (_, img) = source(root, asset)?;
            let img = img.resize(2400, 2400, imageops::FilterType::Lanczos3);
            let image_data = encode(&img, ImageFormat::Jpeg, options)?;
            let object=doc.add_object(Stream::new(dictionary!{"Type"=>"XObject","Subtype"=>"Image","Width"=>img.width() as i64,"Height"=>img.height() as i64,"ColorSpace"=>"DeviceRGB","BitsPerComponent"=>8,"Filter"=>"DCTDecode"},image_data));
            let name = format!("Im{index}");
            images.set(name.clone(), object);
            let (x, top, cw, ch) = layout.cell(index);
            let caption_lines = preview_lines(string(asset, "description", "Grafik"), cw, 10.0, 4)?;
            let caption_h = caption_lines.len() as f32 * 13.0 + 8.0;
            let (ix, iy, iw, ih) = fit_rect(img.width(), img.height(), x, top, cw, ch - caption_h);
            ops.push(Operation::new("q", vec![]));
            ops.push(Operation::new(
                "cm",
                vec![
                    iw.into(),
                    0.into(),
                    0.into(),
                    ih.into(),
                    ix.into(),
                    (layout.height - iy - ih).into(),
                ],
            ));
            ops.push(Operation::new("Do", vec![Object::Name(name.into_bytes())]));
            ops.push(Operation::new("Q", vec![]));
            for (line_index, line) in caption_lines.iter().enumerate() {
                pdf_text(
                    &mut ops,
                    line,
                    x,
                    layout.height - top - ch + caption_h - 13.0 - line_index as f32 * 13.0,
                    10.0,
                )?;
            }
        }
        pdf_text(
            &mut ops,
            &format!("FrameStudio · {} / {}", page_index + 1, page_count),
            28.0,
            14.0,
            8.0,
        )?;
        let content = doc.add_object(Stream::new(
            dictionary! {},
            Content { operations: ops }.encode()?,
        ));
        let page=doc.add_object(dictionary!{"Type"=>"Page","Parent"=>pages_id,"MediaBox"=>vec![0.into(),0.into(),layout.width.into(),layout.height.into()],"Resources"=>dictionary!{"Font"=>dictionary!{"F1"=>font},"XObject"=>images},"Contents"=>content});
        pages.push(Object::Reference(page));
    }
    doc.objects.insert(
        pages_id,
        dictionary! {"Type"=>"Pages","Kids"=>pages.clone(),"Count"=>pages.len() as i64}.into(),
    );
    let catalog = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages_id});
    doc.trailer.set("Root", catalog);
    doc.compress();
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes)?;
    Ok(bytes)
}

const XML_HEAD: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>";
const NS_P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const NS_A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
fn emu(v: f32) -> i64 {
    (v * 12700.0).round() as i64
}

fn text_shape(id: usize, text: &str, x: f32, y: f32, w: f32, h: f32, size: f32) -> String {
    let paragraphs=text.split('\n').map(|line|format!("<a:p><a:r><a:rPr lang=\"de-DE\" sz=\"{}\"/><a:t xml:space=\"preserve\">{}</a:t></a:r><a:endParaRPr lang=\"de-DE\" sz=\"{}\"/></a:p>",(size*100.0) as u32,escape(line),(size*100.0) as u32)).collect::<String>();
    format!(
        "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"Text {id}\"/><p:cNvSpPr txBox=\"1\"/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:noFill/></p:spPr><p:txBody><a:bodyPr wrap=\"square\" lIns=\"0\" tIns=\"0\" rIns=\"0\" bIns=\"0\"><a:normAutofit/></a:bodyPr><a:lstStyle/>{paragraphs}</p:txBody></p:sp>",
        emu(x),
        emu(y),
        emu(w),
        emu(h)
    )
}

fn picture_shape(
    id: usize,
    relationship: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    description: &str,
) -> String {
    format!(
        "<p:pic><p:nvPicPr><p:cNvPr id=\"{id}\" name=\"Grafik {id}\" descr=\"{}\"/><p:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed=\"{relationship}\"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr><a:xfrm><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr></p:pic>",
        escape(description),
        emu(x),
        emu(y),
        emu(w),
        emu(h)
    )
}

fn group_start() -> &'static str {
    "<p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr>"
}

fn pptx_export(root: &Path, project: &Value, assets: &[Value], options: &Value) -> Result<Vec<u8>> {
    let layout = PageLayout::new(options, true)?;
    let title = string(options, "title", string(project, "name", "FrameStudio"));
    let description = string(options, "description", "");
    ensure!(
        title.chars().count() <= 180 && description.chars().count() <= 600,
        "Bitte Titel auf 180 und Beschreibung auf 600 Zeichen begrenzen"
    );
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let mut slide_ids = String::new();
    let mut relationships = String::new();
    let mut overrides = String::new();
    for (pi, chunk) in assets.chunks(layout.per_page()).enumerate() {
        let num = pi + 1;
        let mut shapes = text_shape(2, title, 28.0, 16.0, layout.width - 56.0, 48.0, 24.0);
        shapes.push_str(&text_shape(
            3,
            description,
            28.0,
            68.0,
            layout.width - 56.0,
            28.0,
            11.0,
        ));
        let mut rels = format!(
            "<Relationship Id=\"rIdLayout\" Type=\"{NS_R}/slideLayout\" Target=\"../slideLayouts/slideLayout1.xml\"/>"
        );
        for (i, asset) in chunk.iter().enumerate() {
            let (_, img) = source(root, asset)?;
            let (x, y, w, h) = layout.cell(i);
            let caption =
                preview_lines(string(asset, "description", "Grafik"), w, 12.0, 3)?.join("\n");
            let (ix, iy, iw, ih) = fit_rect(img.width(), img.height(), x, y, w, h - 50.0);
            let rid = format!("rIdImage{i}");
            let image_name = format!("image-{num}-{i}.png");
            zip_file(
                &mut zip,
                &format!("ppt/media/{image_name}"),
                &encode(&img, ImageFormat::Png, options)?,
            )?;
            shapes.push_str(&picture_shape(
                4 + i * 2,
                &rid,
                ix,
                iy,
                iw,
                ih,
                string(asset, "description", "Grafik"),
            ));
            shapes.push_str(&text_shape(
                5 + i * 2,
                &caption,
                x,
                y + h - 46.0,
                w,
                46.0,
                12.0,
            ));
            rels.push_str(&format!("<Relationship Id=\"{rid}\" Type=\"{NS_R}/image\" Target=\"../media/{image_name}\"/>"));
        }
        let slide = format!(
            "{XML_HEAD}<p:sld xmlns:p=\"{NS_P}\" xmlns:a=\"{NS_A}\" xmlns:r=\"{NS_R}\"><p:cSld><p:spTree>{}{shapes}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>",
            group_start()
        );
        zip_file(
            &mut zip,
            &format!("ppt/slides/slide{num}.xml"),
            slide.as_bytes(),
        )?;
        zip_file(
            &mut zip,
            &format!("ppt/slides/_rels/slide{num}.xml.rels"),
            format!("{XML_HEAD}<Relationships xmlns=\"{NS_REL}\">{rels}</Relationships>")
                .as_bytes(),
        )?;
        slide_ids.push_str(&format!(
            "<p:sldId id=\"{}\" r:id=\"rId{num}\"/>",
            255 + num
        ));
        relationships.push_str(&format!(
            "<Relationship Id=\"rId{num}\" Type=\"{NS_R}/slide\" Target=\"slides/slide{num}.xml\"/>"
        ));
        overrides.push_str(&format!("<Override PartName=\"/ppt/slides/slide{num}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>"));
    }
    pptx_structure(&mut zip, layout, &slide_ids, &relationships, &overrides)?;
    Ok(zip.finish()?.into_inner())
}

fn pptx_structure(
    zip: &mut zip::ZipWriter<Cursor<Vec<u8>>>,
    layout: PageLayout,
    slide_ids: &str,
    relationships: &str,
    overrides: &str,
) -> Result<()> {
    let types = format!(
        "{XML_HEAD}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Default Extension=\"png\" ContentType=\"image/png\"/><Override PartName=\"/ppt/presentation.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml\"/><Override PartName=\"/ppt/slideMasters/slideMaster1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml\"/><Override PartName=\"/ppt/slideLayouts/slideLayout1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml\"/><Override PartName=\"/ppt/theme/theme1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.theme+xml\"/>{overrides}</Types>"
    );
    zip_file(zip, "[Content_Types].xml", types.as_bytes())?;
    zip_file(zip,"_rels/.rels",format!("{XML_HEAD}<Relationships xmlns=\"{NS_REL}\"><Relationship Id=\"rId1\" Type=\"{NS_R}/officeDocument\" Target=\"ppt/presentation.xml\"/></Relationships>").as_bytes())?;
    zip_file(zip,"ppt/presentation.xml",format!("{XML_HEAD}<p:presentation xmlns:p=\"{NS_P}\" xmlns:a=\"{NS_A}\" xmlns:r=\"{NS_R}\"><p:sldMasterIdLst><p:sldMasterId id=\"2147483648\" r:id=\"rIdMaster\"/></p:sldMasterIdLst><p:sldIdLst>{slide_ids}</p:sldIdLst><p:sldSz cx=\"{}\" cy=\"{}\"/><p:notesSz cx=\"6858000\" cy=\"9144000\"/><p:defaultTextStyle><a:defPPr><a:defRPr lang=\"de-DE\"/></a:defPPr></p:defaultTextStyle></p:presentation>",emu(layout.width),emu(layout.height)).as_bytes())?;
    zip_file(zip,"ppt/_rels/presentation.xml.rels",format!("{XML_HEAD}<Relationships xmlns=\"{NS_REL}\"><Relationship Id=\"rIdMaster\" Type=\"{NS_R}/slideMaster\" Target=\"slideMasters/slideMaster1.xml\"/>{relationships}</Relationships>").as_bytes())?;
    let color_map = "<p:clrMap accent1=\"accent1\" accent2=\"accent2\" accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" accent6=\"accent6\" bg1=\"lt1\" bg2=\"lt2\" folHlink=\"folHlink\" hlink=\"hlink\" tx1=\"dk1\" tx2=\"dk2\"/>";
    zip_file(zip,"ppt/slideMasters/slideMaster1.xml",format!("{XML_HEAD}<p:sldMaster xmlns:p=\"{NS_P}\" xmlns:a=\"{NS_A}\" xmlns:r=\"{NS_R}\"><p:cSld><p:spTree>{}</p:spTree></p:cSld>{color_map}<p:sldLayoutIdLst><p:sldLayoutId id=\"2147483649\" r:id=\"rIdLayout\"/></p:sldLayoutIdLst><p:txStyles><p:titleStyle/><p:bodyStyle/><p:otherStyle/></p:txStyles></p:sldMaster>",group_start()).as_bytes())?;
    zip_file(zip,"ppt/slideMasters/_rels/slideMaster1.xml.rels",format!("{XML_HEAD}<Relationships xmlns=\"{NS_REL}\"><Relationship Id=\"rIdLayout\" Type=\"{NS_R}/slideLayout\" Target=\"../slideLayouts/slideLayout1.xml\"/><Relationship Id=\"rIdTheme\" Type=\"{NS_R}/theme\" Target=\"../theme/theme1.xml\"/></Relationships>").as_bytes())?;
    zip_file(zip,"ppt/slideLayouts/slideLayout1.xml",format!("{XML_HEAD}<p:sldLayout xmlns:p=\"{NS_P}\" xmlns:a=\"{NS_A}\" xmlns:r=\"{NS_R}\" type=\"blank\" preserve=\"1\"><p:cSld name=\"FrameStudio\"><p:spTree>{}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>",group_start()).as_bytes())?;
    zip_file(zip,"ppt/slideLayouts/_rels/slideLayout1.xml.rels",format!("{XML_HEAD}<Relationships xmlns=\"{NS_REL}\"><Relationship Id=\"rIdMaster\" Type=\"{NS_R}/slideMaster\" Target=\"../slideMasters/slideMaster1.xml\"/></Relationships>").as_bytes())?;
    let colors = [
        ("dk1", "252622"),
        ("lt1", "FFFFFF"),
        ("dk2", "454840"),
        ("lt2", "F5F4EF"),
        ("accent1", "B95732"),
        ("accent2", "597269"),
        ("accent3", "BD985E"),
        ("accent4", "727EA2"),
        ("accent5", "8B6589"),
        ("accent6", "767169"),
        ("hlink", "225FA3"),
        ("folHlink", "6C4987"),
    ]
    .iter()
    .map(|(name, color)| format!("<a:{name}><a:srgbClr val=\"{color}\"/></a:{name}>"))
    .collect::<String>();
    let fill = "<a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill>";
    let line = "<a:ln w=\"12700\"><a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill><a:prstDash val=\"solid\"/></a:ln>";
    zip_file(zip,"ppt/theme/theme1.xml",format!("{XML_HEAD}<a:theme xmlns:a=\"{NS_A}\" name=\"FrameStudio\"><a:themeElements><a:clrScheme name=\"FrameStudio\">{colors}</a:clrScheme><a:fontScheme name=\"FrameStudio\"><a:majorFont><a:latin typeface=\"Aptos\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/></a:majorFont><a:minorFont><a:latin typeface=\"Aptos\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/></a:minorFont></a:fontScheme><a:fmtScheme name=\"FrameStudio\"><a:fillStyleLst>{fill}{fill}{fill}</a:fillStyleLst><a:lnStyleLst>{line}{line}{line}</a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst>{fill}{fill}{fill}</a:bgFillStyleLst></a:fmtScheme></a:themeElements></a:theme>").as_bytes())?;
    Ok(())
}

fn ensure_directory(path: &Path) -> Result<()> {
    if path.exists() {
        ensure!(
            !fs::symlink_metadata(path)?.file_type().is_symlink(),
            "Symbolische Speicherverzeichnisse sind nicht erlaubt"
        );
    } else if let Err(error) = fs::create_dir(path)
        && error.kind() != std::io::ErrorKind::AlreadyExists
    {
        return Err(error.into());
    }
    ensure!(
        !fs::symlink_metadata(path)?.file_type().is_symlink(),
        "Symbolische Speicherverzeichnisse sind nicht erlaubt"
    );
    ensure!(path.is_dir(), "Speicherpfad ist kein Verzeichnis");
    Ok(())
}

pub fn create(root: &Path, project: &Value, assets: &[Value], options: &Value) -> Result<Value> {
    ensure!(
        assets.len() <= 1000,
        "Ein Export darf höchstens 1000 Bilder enthalten"
    );
    let format = string(options, "format", "png");
    ensure!(
        !assets.is_empty() || format == "zip",
        "Bitte mindestens ein Bild auswählen"
    );
    let exports = root.join("exports");
    ensure_directory(&exports)?;
    let id = Uuid::new_v4().to_string();
    let staging = exports.join(format!(".{id}.part"));
    fs::create_dir(&staging)?;
    let result = (|| {
        let files = match format {
            "original" | "png" | "jpeg" | "webp" | "tiff" => {
                image_exports(root, &staging, project, assets, options)?
            }
            "html" => html_export(root, &staging, project, assets, options)?,
            "zip" => {
                atomic(
                    &staging.join("projekt.zip"),
                    &backup(root, project, assets, options)?,
                )?;
                vec!["projekt.zip".into()]
            }
            "pdf" => {
                atomic(
                    &staging.join("grafiken.pdf"),
                    &pdf_export(root, project, assets, options)?,
                )?;
                vec!["grafiken.pdf".into()]
            }
            "pptx" => {
                atomic(
                    &staging.join("grafiken.pptx"),
                    &pptx_export(root, project, assets, options)?,
                )?;
                vec!["grafiken.pptx".into()]
            }
            "sprite" => sprite(root, &staging, assets, options)?,
            _ => bail!("Unbekanntes Exportformat"),
        };
        let metadata = options["include_metadata"].as_bool().unwrap_or(false);
        if metadata && format != "zip" {
            atomic(
                &staging.join("metadata.json"),
                &serde_json::to_vec_pretty(
                    &json!({"version":1,"assets":assets.iter().map(|a|asset_metadata(a,true)).collect::<Vec<_>>()}),
                )?,
            )?;
        }
        let mut final_files = files;
        if metadata && format != "zip" {
            final_files.push("metadata.json".into());
        }
        let destination = exports.join(&id);
        let info = json!({"id":id,"path":destination,"files":final_files,"format":format,"image_count":assets.len(),"scaled":options.get("width").is_some()||options.get("height").is_some(),"notice":"Skalieren erzeugt keine zusätzlichen echten Bilddetails. Lange Dokumentbeschriftungen werden mit … gekürzt; vollständige Texte können als Metadaten mitgegeben werden."});
        atomic(
            &staging.join("export.json"),
            &serde_json::to_vec_pretty(&info)?,
        )?;
        fs::rename(&staging, &destination)?;
        Ok(info)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

fn mime(extension: &str) -> &'static str {
    match extension {
        "png" => "image/png",
        "jpeg" | "jpg" => "image/jpeg",
        "webp" => "image/webp",
        "tiff" | "tif" => "image/tiff",
        "pdf" => "application/pdf",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "html" => "text/html",
        "json" => "application/json",
        _ => "application/zip",
    }
}

pub fn read_export(root: &Path, id: &str) -> Result<Value> {
    Uuid::parse_str(id).context("Ungültige Export-ID")?;
    let manifest_path = safe_file(root, &format!("exports/{id}/export.json"))?;
    let info: Value = serde_json::from_slice(&read_bounded(&manifest_path, 1024 * 1024)?)?;
    let files = info["files"].as_array().context("Exportmanifest fehlt")?;
    ensure!(
        !files.is_empty() && files.len() <= 1002,
        "Exportmanifest ist ungültig"
    );
    let (bytes, name) = if files.len() == 1 {
        let file = files[0].as_str().context("Exportdatei ist ungültig")?;
        ensure!(
            Path::new(file).file_name().and_then(|n| n.to_str()) == Some(file),
            "Ungültiger Exportpfad"
        );
        (
            read_bounded(
                &safe_file(root, &format!("exports/{id}/{file}"))?,
                MAX_ARCHIVE,
            )?,
            file.to_string(),
        )
    } else {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let mut total = 0;
        for file in files {
            let file = file.as_str().context("Exportdatei ist ungültig")?;
            ensure!(
                Path::new(file).file_name().and_then(|n| n.to_str()) == Some(file),
                "Ungültiger Exportpfad"
            );
            let bytes = read_bounded(
                &safe_file(root, &format!("exports/{id}/{file}"))?,
                MAX_ARCHIVE,
            )?;
            total += bytes.len() as u64;
            ensure!(
                total <= MAX_ARCHIVE,
                "Download überschreitet 512 MiB; bitte Exportordner verwenden"
            );
            zip_file(&mut zip, file, &bytes)?;
        }
        (zip.finish()?.into_inner(), "framestudio-export.zip".into())
    };
    let extension = Path::new(&name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("zip");
    Ok(json!({"id":id,"filename":name,"mime":mime(extension),"data_base64":STANDARD.encode(bytes)}))
}

fn checked_archive(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>> {
    ensure!(
        bytes.len() as u64 <= MAX_ARCHIVE,
        "ZIP überschreitet 512 MiB"
    );
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).context("Ungültiges ZIP-Projekt")?;
    ensure!(zip.len() <= 1002, "Zu viele Dateien im ZIP-Projekt");
    let mut entries = BTreeMap::new();
    let mut total = 0u64;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        let name = entry.name()?.to_string();
        ensure!(
            name.len() <= 200
                && !name.contains('\\')
                && !name.contains(':')
                && !name.contains('\0'),
            "Unsicherer ZIP-Pfad"
        );
        ensure!(
            entry.enclosed_name().is_some()
                && Path::new(&name)
                    .components()
                    .all(|p| matches!(p, Component::Normal(_))),
            "Unsicherer ZIP-Pfad"
        );
        ensure!(
            !entry.is_dir() && !entry.is_symlink(),
            "Verzeichnisse und Links sind im Projekt-ZIP nicht erlaubt"
        );
        ensure!(
            name == "manifest.json" || name.starts_with("assets/"),
            "Unbekannter Inhalt im ZIP-Projekt"
        );
        ensure!(entry.size() <= MAX_FILE, "ZIP-Eintrag ist zu groß");
        ensure!(
            entry.compressed_size() > 0 || entry.size() == 0,
            "Ungültige ZIP-Kompression"
        );
        ensure!(
            entry.size()
                <= entry
                    .compressed_size()
                    .saturating_mul(2000)
                    .max(1024 * 1024),
            "Verdächtige ZIP-Kompressionsrate"
        );
        total = total
            .checked_add(entry.size())
            .context("ZIP-Größe überschritten")?;
        ensure!(total <= MAX_ARCHIVE, "Entpacktes ZIP überschreitet 512 MiB");
        let mut data = Vec::new();
        entry.by_ref().take(MAX_FILE + 1).read_to_end(&mut data)?;
        ensure!(
            data.len() as u64 == entry.size() && data.len() as u64 <= MAX_FILE,
            "Unvollständiger ZIP-Eintrag"
        );
        ensure!(
            entries.insert(name, data).is_none(),
            "Doppelte ZIP-Pfade sind nicht erlaubt"
        );
    }
    Ok(entries)
}

pub fn restore(root: &Path, bytes: &[u8]) -> Result<Value> {
    let mut entries = checked_archive(bytes)?;
    let manifest = entries
        .remove("manifest.json")
        .context("Projektmanifest fehlt")?;
    ensure!(
        manifest.len() <= 8 * 1024 * 1024,
        "Projektmanifest ist zu groß"
    );
    let mut data: Value =
        serde_json::from_slice(&manifest).context("Projektmanifest ist ungültig")?;
    ensure!(
        data["version"] == 1 && data["application"] == "FrameStudio",
        "Unbekannte Projektversion"
    );
    ensure!(data["project"].is_object(), "Projekt fehlt im Manifest");
    for field in ["styles", "characters", "jobs"] {
        if data[field].is_null() {
            data[field] = json!([]);
        }
        ensure!(data[field].is_array(), "Ungültige Projektliste: {field}");
    }
    let mut assets = data["assets"]
        .as_array()
        .context("Assetliste fehlt")?
        .clone();
    ensure!(assets.len() <= 1000, "Zu viele Bilder im Projekt");
    let assets_dir = root.join("assets");
    ensure_directory(&assets_dir)?;
    let group = format!("restored-{}", Uuid::new_v4());
    let stage = root.join(format!(".{group}.part"));
    fs::create_dir(&stage)?;
    let result = (|| {
        let mut used = HashSet::new();
        for (i, asset) in assets.iter_mut().enumerate() {
            let id = string(asset, "id", "");
            ensure!(
                !id.is_empty() && id.len() <= 128 && used.insert(id.to_string()),
                "Ungültige oder doppelte Asset-ID"
            );
            let path = string(asset, "path", "").to_string();
            let bytes = entries.remove(&path).context("Assetdatei fehlt im ZIP")?;
            let img = decode_image(&bytes)?;
            let format = image::guess_format(&bytes)?;
            let ext = format.extensions_str()[0];
            let file = format!("{i:04}.{ext}");
            let thumb = format!("{i:04}-thumb.png");
            atomic(&stage.join(&file), &bytes)?;
            atomic(
                &stage.join(&thumb),
                &encode(&img.thumbnail(384, 384), ImageFormat::Png, &json!({}))?,
            )?;
            *asset = asset_metadata(asset, true);
            asset["path"] = json!(format!("assets/{group}/{file}"));
            asset["thumbnail_path"] = json!(format!("assets/{group}/{thumb}"));
            asset["width"] = json!(img.width());
            asset["height"] = json!(img.height());
            asset["format"] = json!(ext);
            asset["mime"] = json!(format.to_mime_type());
            asset["sha256"] = json!(
                Sha256::digest(&bytes)
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            );
        }
        ensure!(entries.is_empty(), "ZIP enthält nicht zugeordnete Dateien");
        fs::rename(&stage, assets_dir.join(&group))?;
        data["assets"] = json!(assets);
        data["project"] = curated(
            &data["project"],
            &[
                "id",
                "name",
                "description",
                "created_at",
                "aspect_ratio",
                "style_id",
                "preferences",
                "budget_microusd",
                "archived",
                "prompt_template",
            ],
        );
        data["styles"] = domain_list(&data["styles"], "style");
        data["characters"] = domain_list(&data["characters"], "character");
        data["jobs"] = domain_list(&data["jobs"], "job");
        Ok(curated(
            &data,
            &["project", "assets", "styles", "characters", "jobs"],
        ))
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&stage);
    }
    result
}

/// Copy a completed export to an explicitly configured directory, retaining the
/// canonical internal copy for authenticated API download.
pub fn copy_to_directory(root: &Path, result: &Value, directory: &Path) -> Result<Value> {
    ensure!(directory.is_dir(), "Das Exportverzeichnis fehlt");
    ensure!(
        !fs::symlink_metadata(directory)?.file_type().is_symlink(),
        "Exportverzeichnis darf kein symbolischer Link sein"
    );
    let base = directory.canonicalize()?;
    let id = string(result, "id", "");
    Uuid::parse_str(id)?;
    let destination = base.join(format!("FrameStudio-{id}"));
    ensure!(!destination.exists(), "Exportziel existiert bereits");
    let stage = base.join(format!(".framestudio-{}.part", Uuid::new_v4()));
    fs::create_dir(&stage)?;
    let attempt = (|| {
        let files = result["files"]
            .as_array()
            .context("Exportdateiliste fehlt")?;
        for file in files {
            let name = file.as_str().context("Ungültige Exportdatei")?;
            ensure!(
                Path::new(name).file_name().and_then(|s| s.to_str()) == Some(name),
                "Ungültiger Exportdateiname"
            );
            let bytes = read_bounded(
                &safe_file(root, &format!("exports/{id}/{name}"))?,
                MAX_ARCHIVE,
            )?;
            atomic(&stage.join(name), &bytes)?;
        }
        fs::rename(&stage, &destination)?;
        let mut published = result.clone();
        published["path"] = json!(destination);
        Ok(published)
    })();
    if attempt.is_err() {
        let _ = fs::remove_dir_all(stage);
    }
    attempt
}
