use framestudio::exports;
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use serde_json::{Value, json};
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::Path;
use tempfile::TempDir;

fn fixture(root: &Path) -> (Value, Vec<Value>) {
    fs::create_dir_all(root.join("assets")).unwrap();
    let project = json!({"id":"project-fixture","name":"Prüfprojekt","description":"Synthetische Exportprüfung","backup_styles":[],"backup_characters":[],"backup_jobs":[]});
    let assets=(0..3).map(|i| {
        let image=DynamicImage::ImageRgba8(RgbaImage::from_fn(320,240,|x,y|if x<80 {Rgba([30,100,180,0])}else{Rgba([(i*60+50) as u8,(x%255) as u8,(y%255) as u8,255])}));
        let mut bytes=Cursor::new(Vec::new());image.write_to(&mut bytes,ImageFormat::Png).unwrap();
        let path=format!("assets/fixture-{i}.png");fs::write(root.join(&path),bytes.into_inner()).unwrap();
        json!({"id":format!("asset-{i}"),"project_id":"project-fixture","path":path,"description":format!("Synthetische Grafik {}: Grüner Würfel – ÄÖÜ äöü ß",i+1),"width":320,"height":240,"format":"png","prompt":"HISTORICAL PROMPT","api_key":"NEVER_EXPORT","snapshot":{"token":"NEVER_EXPORT","scene":"test"}})
    }).collect();
    (project, assets)
}

fn output(root: &Path, result: &Value, index: usize) -> Vec<u8> {
    fs::read(
        root.join("exports")
            .join(result["id"].as_str().unwrap())
            .join(result["files"][index].as_str().unwrap()),
    )
    .unwrap()
}

#[test]
fn every_raster_export_decodes_and_preserves_dimensions() {
    let tmp = TempDir::new().unwrap();
    let (project, assets) = fixture(tmp.path());
    for format in ["png", "jpeg", "webp", "tiff", "original"] {
        let result=exports::create(tmp.path(),&project,&assets,&json!({"format":format,"width":180,"height":180,"fit":"contain","background":"#fafafa"})).unwrap();
        let bytes = output(tmp.path(), &result, 0);
        let img = image::load_from_memory(&bytes).unwrap();
        assert_eq!(img.width(), if format == "original" { 320 } else { 180 });
        assert_eq!(img.height(), if format == "original" { 240 } else { 180 });
        if format == "original" {
            assert_eq!(
                bytes,
                fs::read(tmp.path().join("assets/fixture-0.png")).unwrap()
            );
        }
        if format == "png" || format == "webp" || format == "tiff" {
            assert_eq!(img.to_rgba8().get_pixel(0, 0)[3], 0);
        }
        if format == "jpeg" {
            let color = img.to_rgb8();
            assert!(color.get_pixel(0, 0)[0] > 240);
        }
    }
}

#[test]
fn html_escapes_scripts_and_stays_offline() {
    let tmp = TempDir::new().unwrap();
    let (project, mut assets) = fixture(tmp.path());
    assets[0]["description"] = json!("<script>alert(1)</script><img src=https://evil.invalid>");
    let result = exports::create(
        tmp.path(),
        &project,
        &assets,
        &json!({"format":"html","title":"<b>Werkstatt</b>"}),
    )
    .unwrap();
    let html = String::from_utf8(output(tmp.path(), &result, 0)).unwrap();
    assert!(!html.contains("<script>"));
    assert!(html.contains("&lt;script&gt;"));
    assert!(html.contains("default-src 'none'"));
    assert!(!html.contains("<img src=https://"));
}

#[test]
fn pdf_is_parseable_has_unicode_text_and_three_pages() {
    let tmp = TempDir::new().unwrap();
    let (project, assets) = fixture(tmp.path());
    let result = exports::create(
        tmp.path(),
        &project,
        &assets,
        &json!({"format":"pdf","title":"Ästhetik – Prüfung","layout":"single"}),
    )
    .unwrap();
    let bytes = output(tmp.path(), &result, 0);
    let doc = lopdf::Document::load_mem(&bytes).unwrap();
    assert_eq!(doc.get_pages().len(), 3);
    let text = doc.extract_text(&[1]).unwrap();
    assert!(text.contains("Ästhetik"), "{text}");
    assert!(text.contains("Grüner Würfel"), "{text}");
}

#[test]
fn pptx_has_real_slides_images_and_editable_text() {
    let tmp = TempDir::new().unwrap();
    let (project, assets) = fixture(tmp.path());
    let result = exports::create(
        tmp.path(),
        &project,
        &assets,
        &json!({"format":"pptx","title":"Ästhetik – Prüfung","layout":"single"}),
    )
    .unwrap();
    let bytes = output(tmp.path(), &result, 0);
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut slide = String::new();
    zip.by_name("ppt/slides/slide1.xml")
        .unwrap()
        .read_to_string(&mut slide)
        .unwrap();
    assert!(slide.contains("<a:t"));
    assert!(slide.contains("Ästhetik – Prüfung"));
    assert!(slide.contains("<p:pic>"));
    assert!(zip.by_name("ppt/slides/slide3.xml").is_ok());
    assert!(zip.by_name("ppt/slides/slide4.xml").is_err());
    let mut png = Vec::new();
    zip.by_name("ppt/media/image-1-0.png")
        .unwrap()
        .read_to_end(&mut png)
        .unwrap();
    assert!(image::load_from_memory(&png).is_ok());
}

#[test]
fn backup_roundtrip_preserves_originals_and_excludes_secrets() {
    let tmp = TempDir::new().unwrap();
    let (project, assets) = fixture(tmp.path());
    let result = exports::create(
        tmp.path(),
        &project,
        &assets,
        &json!({"format":"zip","include_metadata":true}),
    )
    .unwrap();
    let bytes = output(tmp.path(), &result, 0);
    let mut zip = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
    let mut manifest = String::new();
    zip.by_name("manifest.json")
        .unwrap()
        .read_to_string(&mut manifest)
        .unwrap();
    assert!(!manifest.contains("NEVER_EXPORT"));
    assert!(manifest.contains("HISTORICAL PROMPT"));
    let restored = exports::restore(tmp.path(), &bytes).unwrap();
    assert_eq!(restored["assets"].as_array().unwrap().len(), 3);
    for (i, asset) in restored["assets"].as_array().unwrap().iter().enumerate() {
        assert_eq!(
            fs::read(tmp.path().join(asset["path"].as_str().unwrap())).unwrap(),
            fs::read(tmp.path().join(format!("assets/fixture-{i}.png"))).unwrap()
        );
        assert!(
            tmp.path()
                .join(asset["thumbnail_path"].as_str().unwrap())
                .is_file()
        );
    }
}

#[test]
fn sharing_backup_excludes_prompts_and_history() {
    let tmp = TempDir::new().unwrap();
    let (project, assets) = fixture(tmp.path());
    let result = exports::create(tmp.path(), &project, &assets, &json!({"format":"zip"})).unwrap();
    let bytes = output(tmp.path(), &result, 0);
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut text = String::new();
    zip.by_name("manifest.json")
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    assert!(!text.contains("HISTORICAL"));
    assert!(!text.contains("NEVER_EXPORT"));
}

fn malicious_zip(name: &str) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file(name, zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"bad").unwrap();
    zip.finish().unwrap().into_inner()
}

#[test]
fn rejects_traversal_missing_images_and_unsupported_formats() {
    let tmp = TempDir::new().unwrap();
    let (project, mut assets) = fixture(tmp.path());
    for name in [
        "../escape",
        "/tmp/escape",
        "assets/../../escape",
        "assets\\escape",
        "assets/C:escape",
    ] {
        assert!(
            exports::restore(tmp.path(), &malicious_zip(name)).is_err(),
            "{name}"
        );
    }
    assets[0]["path"] = json!("../outside.png");
    assert!(exports::create(tmp.path(), &project, &assets, &json!({"format":"png"})).is_err());
    assert!(exports::decode_image(b"<svg onload='evil'/>").is_err());
    assert!(exports::read_export(tmp.path(), "../escape").is_err());
}

#[cfg(unix)]
#[test]
fn rejects_symlink_sources() {
    let tmp = TempDir::new().unwrap();
    let (project, mut assets) = fixture(tmp.path());
    std::os::unix::fs::symlink(
        tmp.path().join("assets/fixture-0.png"),
        tmp.path().join("assets/link.png"),
    )
    .unwrap();
    assets[0]["path"] = json!("assets/link.png");
    assert!(exports::create(tmp.path(), &project, &assets, &json!({"format":"png"})).is_err());
}

#[test]
fn names_are_portable_and_parallel_exports_do_not_collide() {
    assert_eq!(exports::safe_name("CON"), "_CON");
    assert!(!exports::safe_name("../../a:b?c").contains('/'));
    let tmp = TempDir::new().unwrap();
    let (project, assets) = fixture(tmp.path());
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    exports::create(
                        tmp.path(),
                        &project,
                        &assets,
                        &json!({"format":"png","naming_template":"same"}),
                    )
                    .unwrap()
                })
            })
            .collect();
        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let ids: std::collections::HashSet<_> =
            results.iter().map(|v| v["id"].as_str().unwrap()).collect();
        assert_eq!(ids.len(), 8);
        for result in &results {
            assert_eq!(result["files"].as_array().unwrap().len(), 3);
        }
    });
    assert!(
        exports::filename(
            &project,
            &assets[0],
            &json!({"naming_template":"{nope}"}),
            1
        )
        .is_err()
    );
}

#[test]
fn sprite_metadata_order_and_no_crop() {
    let tmp = TempDir::new().unwrap();
    let (project, assets) = fixture(tmp.path());
    let result = exports::create(
        tmp.path(),
        &project,
        &assets,
        &json!({"format":"sprite","cell_width":128,"cell_height":128,"columns":2,"padding":8}),
    )
    .unwrap();
    let img = image::load_from_memory(&output(tmp.path(), &result, 0)).unwrap();
    assert_eq!(img.width(), 280);
    assert_eq!(img.height(), 280);
    let manifest: Value = serde_json::from_slice(&output(tmp.path(), &result, 1)).unwrap();
    assert_eq!(manifest["frames"][2]["asset_id"], "asset-2");
    assert_eq!(manifest["frames"][2]["trimmed"], false);
}

#[test]
fn bad_dimensions_fail_and_atomic_staging_is_removed() {
    let tmp = TempDir::new().unwrap();
    let (project, assets) = fixture(tmp.path());
    assert!(
        exports::create(
            tmp.path(),
            &project,
            &assets,
            &json!({"format":"png","width":0})
        )
        .is_err()
    );
    assert!(
        fs::read_dir(tmp.path().join("exports"))
            .unwrap()
            .next()
            .is_none()
    );
    assert!(
        exports::create(
            tmp.path(),
            &project,
            &assets,
            &json!({"format":"sprite","columns":9999,"cell_width":16384,"cell_height":16384})
        )
        .is_err()
    );
}

#[test]
#[ignore = "writes local visual acceptance artifacts"]
fn visual_proof() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join(".artifacts/export-proof");
    fs::create_dir_all(&root).unwrap();
    let (project, assets) = fixture(&root);
    for layout in ["single", "contact"] {
        for format in ["pdf", "pptx"] {
            let result=exports::create(&root,&project,&assets,&json!({"format":format,"layout":layout,"title":"FrameStudio · Exportprüfung","description":"Drei synthetische Bilder. Seitenverhältnis und Transparenz prüfen. ÄÖÜ äöü ß."})).unwrap();
            fs::copy(
                root.join("exports")
                    .join(result["id"].as_str().unwrap())
                    .join(result["files"][0].as_str().unwrap()),
                root.join(format!("{layout}.{format}")),
            )
            .unwrap();
        }
    }
}

#[test]
fn configured_directory_gets_complete_atomic_copy() {
    let tmp = TempDir::new().unwrap();
    let destination = TempDir::new().unwrap();
    let (project, assets) = fixture(tmp.path());
    let result = exports::create(tmp.path(), &project, &assets, &json!({"format":"png"})).unwrap();
    let published = exports::copy_to_directory(tmp.path(), &result, destination.path()).unwrap();
    assert!(Path::new(published["path"].as_str().unwrap()).is_dir());
    assert!(
        !Path::new(published["path"].as_str().unwrap())
            .join("export.json")
            .exists()
    );
    assert!(exports::read_export(tmp.path(), result["id"].as_str().unwrap()).is_ok());
    assert!(exports::copy_to_directory(tmp.path(), &result, destination.path()).is_err());
}

fn custom_manifest(manifest: &Value) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file("manifest.json", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(&serde_json::to_vec(manifest).unwrap())
        .unwrap();
    zip.finish().unwrap().into_inner()
}

#[test]
fn hostile_scalar_manifest_entities_return_errors_without_panic() {
    let tmp = TempDir::new().unwrap();
    for value in [
        json!({"version":1,"application":"FrameStudio","project":false,"assets":[]}),
        json!({"version":1,"application":"FrameStudio","project":{},"assets":["bad"]}),
        json!({"version":1,"application":"FrameStudio","project":{},"assets":[],"jobs":3}),
    ] {
        assert!(exports::restore(tmp.path(), &custom_manifest(&value)).is_err());
    }
}

#[test]
fn backup_whitelists_domain_records_and_keeps_usage_counts() {
    let tmp = TempDir::new().unwrap();
    let (mut project, assets) = fixture(tmp.path());
    project["backup_jobs"] = json!([{"id":"j","created_at":"2026-10-31","status":"completed","usage":{"total_input_tokens":42,"api_key":"NEVER_EXPORT"},"actual_microusd":123,"unknown_sensitive_field":"NEVER_EXPORT"}]);
    project["backup_styles"] = json!([{"id":"s","name":"Stil","blocks":{"medium":"Tinte","api_key":"NEVER_EXPORT"},"arbitrary":"NEVER_EXPORT"}]);
    let result = exports::create(
        tmp.path(),
        &project,
        &assets,
        &json!({"format":"zip","include_metadata":true}),
    )
    .unwrap();
    let restored = exports::restore(tmp.path(), &output(tmp.path(), &result, 0)).unwrap();
    let text = serde_json::to_string(&restored).unwrap();
    assert!(!text.contains("NEVER_EXPORT"));
    assert_eq!(restored["jobs"][0]["usage"]["total_input_tokens"], 42);
    assert!(restored["assets"][0]["sha256"].as_str().unwrap().len() == 64);
    assert_eq!(restored["assets"][0]["mime"], "image/png");
}
