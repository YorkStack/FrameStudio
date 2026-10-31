//! Documented wire contract and local response/export processing; no live request.
use base64::{Engine, engine::general_purpose::STANDARD};
use framestudio::{core::Core, provider};
use image::{DynamicImage, ImageFormat};
use serde_json::json;
use std::io::Cursor;

#[test]
fn gemini_output_uses_documented_jpeg_while_png_references_remain_valid() {
    let snapshot = json!({"prompt":"Synthetic style preview","parameters":{"aspect_ratio":"16:9","resolution":"2K"}});
    let body = provider::request_body(
        &snapshot,
        &[json!({"mime":"image/png","data_base64":"synthetic-inline-reference"})],
    );
    assert_eq!(body["model"], provider::MODEL);
    assert_eq!(
        body["response_format"],
        json!({"type":"image","mime_type":"image/jpeg","aspect_ratio":"16:9","image_size":"2K"})
    );
    assert_eq!(body["store"], false);
    assert_eq!(body["input"][1]["mime_type"], "image/png");
    assert!(body.get("tools").is_none());
    assert!(body.get("key").is_none());
    assert!(
        body["response_format"].get("delivery").is_none(),
        "The live Gemini endpoint rejects an explicit image delivery mode"
    );
}

#[tokio::test]
async fn jpeg_response_preserves_original_and_exports_real_png() {
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::new_rgb8(96, 64)
        .write_to(&mut bytes, ImageFormat::Jpeg)
        .unwrap();
    let original = bytes.into_inner();
    let response = json!({"id":"synthetic-image-response","status":"completed","steps":[{"type":"model_output","content":[{"type":"image","mime_type":"image/jpeg","data":STANDARD.encode(&original)}]}]});
    let parsed = provider::parse_response(&response);
    assert!(parsed.error.is_none());
    assert_eq!(parsed.images.len(), 1);
    assert_eq!(parsed.images[0], original);
    assert!(parsed.cost.is_none());
    let root = tempfile::tempdir().unwrap();
    let core = Core::open(root.path().into()).unwrap();
    let project = core
        .call(
            "project.save",
            json!({"name":"Synthetic JPEG contract"}),
            false,
        )
        .await
        .unwrap();
    let asset = core.call("asset.import", json!({"project_id":project["id"],"description":"Synthetic Gemini response","data_base64":STANDARD.encode(&parsed.images[0])}), false).await.unwrap();
    assert_eq!(asset["mime"], "image/jpeg");
    let saved = core
        .call("asset.read", json!({"id":asset["id"]}), false)
        .await
        .unwrap();
    assert_eq!(
        STANDARD
            .decode(saved["data_base64"].as_str().unwrap())
            .unwrap(),
        original
    );
    let exported = core
        .call(
            "export.create",
            json!({"project_id":project["id"],"asset_ids":[asset["id"]],"format":"png"}),
            false,
        )
        .await
        .unwrap();
    // The exporter may return a ZIP for selections; inspect the actual output files.
    let output = root
        .path()
        .join("exports")
        .join(exported["id"].as_str().unwrap());
    let png = std::fs::read_dir(output)
        .unwrap()
        .filter_map(Result::ok)
        .find(|entry| entry.path().extension().is_some_and(|e| e == "png"))
        .unwrap();
    let converted = std::fs::read(png.path()).unwrap();
    assert_eq!(image::guess_format(&converted).unwrap(), ImageFormat::Png);
    assert_eq!(image::load_from_memory(&converted).unwrap().width(), 96);
}
