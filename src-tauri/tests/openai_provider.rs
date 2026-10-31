use base64::{Engine, engine::general_purpose::STANDARD};
use framestudio::{
    core::Core,
    provider::{self, openai},
};
use image::{DynamicImage, ImageFormat};
use serde_json::{Value, json};
use std::io::Cursor;
use tempfile::tempdir;

fn parameters() -> Value {
    json!({"provider":"openai","model":openai::SUNBURST,"aspect_ratio":"1:1","resolution":"1K","quality":"medium"})
}
fn snapshot() -> Value {
    json!({"parameters":parameters(),"prompt":"Synthetic test image","price_snapshot":openai::price_snapshot()})
}
fn image_bytes(format: ImageFormat) -> Vec<u8> {
    let mut output = Cursor::new(Vec::new());
    DynamicImage::new_rgba8(3, 2)
        .write_to(&mut output, format)
        .unwrap();
    output.into_inner()
}
fn usage() -> Value {
    json!({"input_tokens":300,"input_tokens_details":{"text_tokens":100,"image_tokens":200},"output_tokens":500,"total_tokens":800})
}

#[test]
fn pinned_models_parameters_and_paid_extras_are_bounded() {
    let mut args = parameters();
    for model in [openai::SUNBURST, openai::FLARE] {
        args["model"] = json!(model);
        for quality in ["low", "medium", "high", "xhigh", "max"] {
            args["quality"] = json!(quality);
            assert!(provider::validate(&args).is_ok());
        }
    }
    for (field, value) in [
        ("model", json!("gpt-6")),
        ("provider", json!("gemini")),
        ("quality", json!("auto")),
        ("quality", json!(3)),
        ("quality", Value::Null),
        ("aspect_ratio", json!("16:9")),
        ("resolution", json!("4K")),
        ("endpoint", json!("https://evil.invalid")),
        ("seed", json!(3)),
    ] {
        let mut args = parameters();
        args[field] = value;
        assert!(provider::validate(&args).is_err(), "field {field}");
    }
    let mut gemini =
        json!({"provider":"gemini","model":provider::MODEL,"aspect_ratio":"1:1","resolution":"1K"});
    gemini["quality"] = json!("high");
    assert!(provider::validate(&gemini).is_err());
}

#[test]
fn direct_generation_body_is_documented_and_has_no_hidden_extras() {
    let value = openai::request_body(&snapshot(), &[]).unwrap();
    assert_eq!(
        value,
        json!({"model":openai::SUNBURST,"prompt":"Synthetic test image","n":1,"size":"1024x1024","quality":"medium","output_format":"png","background":"opaque","moderation":"auto"})
    );
    assert_eq!(
        openai::endpoint(false),
        "https://api.openai.com/v1/images/generations"
    );
    assert_eq!(
        openai::endpoint(true),
        "https://api.openai.com/v1/images/edits"
    );
    for (ratio, size) in [("3:2", "1536x1024"), ("2:3", "1024x1536")] {
        let mut s = snapshot();
        s["parameters"]["aspect_ratio"] = json!(ratio);
        assert_eq!(openai::request_body(&s, &[]).unwrap()["size"], size);
    }
}

#[test]
fn references_are_verified_inline_images_and_tiff_is_reencoded() {
    let refs = [
        json!({"mime":"image/evil","data_base64":STANDARD.encode(image_bytes(ImageFormat::Tiff))}),
    ];
    let value = openai::request_body(&snapshot(), &refs).unwrap();
    let url = value["images"][0]["image_url"].as_str().unwrap();
    let encoded = url.strip_prefix("data:image/png;base64,").unwrap();
    assert_eq!(
        provider::validate_image(&STANDARD.decode(encoded).unwrap())
            .unwrap()
            .1,
        ImageFormat::Png
    );
    assert!(openai::request_body(&snapshot(), &vec![refs[0].clone(); 9]).is_err());
    assert!(
        openai::request_body(
            &snapshot(),
            &[json!({"data_base64":"https://evil.invalid/image.png"})]
        )
        .is_err()
    );
}

#[test]
fn usage_is_integer_historical_and_unknown_on_missing_or_inconsistent_fields() {
    let price = openai::price_snapshot();
    assert_eq!(provider::usage_cost_at(&usage(), &price), Some(17100));
    let mut historical = price.clone();
    historical["image_output_microusd_per_million"] = json!(60_000_000);
    assert_eq!(provider::usage_cost_at(&usage(), &historical), Some(32100));
    assert_eq!(provider::usage_cost_at(&usage(), &json!({})), None);
    for broken in [
        json!({}),
        json!({"input_tokens":1}),
        json!({"input_tokens":300,"input_tokens_details":{"text_tokens":100,"image_tokens":200},"output_tokens":500,"total_tokens":801}),
    ] {
        assert_eq!(openai::usage_cost_at(&broken, &price), None);
    }
    let mut with_details = usage();
    with_details["output_tokens_details"] = json!({"image_tokens":500,"text_tokens":0});
    assert_eq!(openai::usage_cost_at(&with_details, &price), Some(17100));
    with_details["output_tokens_details"] = json!({"image_tokens":499,"text_tokens":1});
    assert_eq!(openai::usage_cost_at(&with_details, &price), None);
    let mut negative = usage();
    negative["input_tokens_details"]["text_tokens"] = json!(-1);
    assert_eq!(openai::usage_cost_at(&negative, &price), None);
}

#[test]
fn output_estimates_match_official_calculator_and_include_inputs() {
    // Calculator on developers.openai.com, checked 2026-10-31.
    let square = [196, 439, 1756, 3122, 7024];
    let landscape = [158, 343, 1372, 2459, 5488];
    for (index, quality) in ["low", "medium", "high", "xhigh", "max"].iter().enumerate() {
        for (ratio, expected) in [
            ("1:1", square[index]),
            ("3:2", landscape[index]),
            ("2:3", landscape[index]),
        ] {
            let mut args = parameters();
            args["quality"] = json!(quality);
            args["aspect_ratio"] = json!(ratio);
            let estimate = provider::estimate(&args, "abc", 1, 3).unwrap();
            assert_eq!(
                estimate["estimated_output_tokens"], expected,
                "{ratio}, {quality}"
            );
            let per = expected * 30 + 3 * 5 + 8192 * 8;
            assert_eq!(estimate["per_image_microusd"], per);
            assert_eq!(estimate["total_microusd"], per * 3);
            assert_eq!(estimate["price_snapshot"]["provider"], "openai");
        }
    }
}

#[test]
fn response_decodes_images_and_preserves_usage_on_failures() {
    let response = json!({"data":[{"b64_json":STANDARD.encode(image_bytes(ImageFormat::Png))}],"usage":usage()});
    let good = openai::parse_response_at(&response, &openai::price_snapshot());
    assert_eq!(good.images.len(), 1);
    assert!(good.error.is_none());
    assert_eq!(good.cost, Some(17100));
    for data in [
        json!([]),
        json!([{"url":"https://evil.invalid"}]),
        json!([{"b64_json":"broken!"}]),
        json!([{"b64_json":STANDARD.encode(b"not an image")}]),
        json!([{}, {}]),
    ] {
        let bad = openai::parse_response_at(
            &json!({"data":data,"usage":usage()}),
            &openai::price_snapshot(),
        );
        assert!(bad.images.is_empty());
        assert!(bad.error.is_some());
        assert_eq!(bad.cost, Some(17100));
    }
    let unknown =
        openai::parse_response_at(&json!({"data":response["data"]}), &openai::price_snapshot());
    assert_eq!(unknown.images.len(), 1);
    assert_eq!(unknown.cost, None);
}

#[tokio::test]
async fn shared_core_defaults_quality_reserves_budget_and_blocks_automation() {
    let dir = tempdir().unwrap();
    let core = Core::open(dir.path().to_path_buf()).unwrap();
    let project = core
        .call(
            "project.save",
            json!({"name":"Synthetic OpenAI","budget_microusd":1_000_000}),
            false,
        )
        .await
        .unwrap();
    let args = json!({"project_id":project["id"],"description":"Synthetic prompt","provider":"openai","model":openai::SUNBURST,"aspect_ratio":"1:1","resolution":"1K","count":1,"idempotency_key":"openai-test"});
    assert!(
        core.call("job.submit", args.clone(), true)
            .await
            .unwrap_err()
            .to_string()
            .contains("nicht lokal freigegeben")
    );
    let estimate = core
        .call("cost.estimate", args.clone(), false)
        .await
        .unwrap();
    let submitted = core.call("job.submit", args.clone(), false).await.unwrap();
    let job = &submitted["jobs"][0];
    assert_eq!(job["snapshot"]["parameters"]["quality"], "medium");
    assert_eq!(job["snapshot"]["price_snapshot"]["provider"], "openai");
    assert_eq!(job["reserved_microusd"], estimate["total_microusd"]);
    assert_eq!(
        core.call("job.submit", args.clone(), false).await.unwrap()["jobs"][0]["id"],
        job["id"]
    );
    core.call("job.cancel", json!({"id":job["id"]}), false)
        .await
        .unwrap();
    let mut low_budget = project.clone();
    low_budget["budget_microusd"] = json!(1);
    core.call("project.save", low_budget, false).await.unwrap();
    let mut next = args.clone();
    next["idempotency_key"] = json!("budget-reject");
    assert!(
        core.call("job.submit", next, false)
            .await
            .unwrap_err()
            .to_string()
            .contains("Projektbudget")
    );
    for value in [json!(17), Value::Null, json!("auto")] {
        let mut invalid = args.clone();
        invalid["quality"] = value;
        assert!(core.call("prompt.preview", invalid, false).await.is_err());
    }
}

#[test]
fn all_documented_output_formats_decode_without_reencoding() {
    for format in [ImageFormat::Png, ImageFormat::Jpeg, ImageFormat::WebP] {
        let mut buffer = Cursor::new(Vec::new());
        DynamicImage::new_rgb8(32, 24)
            .write_to(&mut buffer, format)
            .unwrap();
        let original = buffer.into_inner();
        let response = json!({"data":[{"b64_json":STANDARD.encode(&original)}]});
        let result = openai::parse_response_at(&response, &openai::price_snapshot());
        assert!(result.error.is_none());
        assert_eq!(result.images, vec![original]);
        let (decoded, detected) = provider::validate_image(&result.images[0]).unwrap();
        assert_eq!(detected, format);
        assert_eq!((decoded.width(), decoded.height()), (32, 24));
        assert_eq!(result.cost, None);
    }
}
