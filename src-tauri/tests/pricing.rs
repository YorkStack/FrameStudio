//! Synthetic source fixtures mirror table structure, not production credentials or image requests.
use framestudio::{pricing, provider};
use serde_json::{Value, json};
fn google(input: &str, image: &str) -> String {
    format!(
        r#"<html><h2 id="other">Wrong model</h2><table><tr><td>$999</td></tr></table>
    <h2 id="gemini-nano-banana-2.1">Nano Banana 2.1</h2><h3>Standard</h3>
    <table><thead><tr><th></th><th>Free Tier</th><th>Paid Tier, per 1M tokens in USD</th></tr></thead><tbody>
    <tr><td>Input price</td><td>Not available</td><td>{input} (text/image/video)</td></tr>
    <tr><td>Output price</td><td>Not available</td><td>$7.50 (text and thinking)<br>{image} (images)<br>Equivalent to $0.0336 per 1K image</td></tr></tbody></table>
    <h3>Batch</h3><table><tbody><tr><td>Input price</td><td>Not available</td><td>$0.75 (text/image/video)</td></tr></tbody></table>
    <p>Output images at 1K (1024x1024px) consume 1120 tokens and are equivalent to $0.0336 per image.
    Output images at 2K (2048x2048px) consume 1680 tokens. Output images at 4K (4096x4096px) consume 3780 tokens.</p><h2>Other model</h2></html>"#
    )
}
fn openai() -> String {
    let mut tables = String::new();
    for tier in ["standard", "batch"] {
        let output = if tier == "standard" {
            "$30.00"
        } else {
            "$15.00"
        };
        let mut rows = String::new();
        for name in ["gpt-image-2.5-sunburst", "gpt-image-2.5-flare"] {
            rows.push_str(&format!("<tr><td rowspan='2'>{name}</td><td>Image</td><td>$8.00</td><td>{output}</td><td>$2.00</td></tr><tr><td>Text</td><td>$5.00</td><td>-</td><td>$1.25</td></tr>"));
        }
        tables.push_str(&format!("<div data-content-switcher-pane='true' data-value='{tier}'><table><thead><tr><th>Model</th><th>Modality</th><th>Input</th><th>Output</th><th>Cached input</th></tr></thead><tbody>{rows}</tbody></table></div>"));
    }
    format!(
        "<section class='pricing-multimodal-subsection'><h3 id='image-generation'>Image generation models</h3><small class='pricing-switcher-meta'>Prices per 1M tokens.</small>{tables}</section>"
    )
}
fn anthropic() -> &'static str {
    "All prices are in USD.\n## Model pricing\n| Model | Base input tokens | 5m cache writes | 1h cache writes | Cache hits and refreshes | Output tokens |\n| --- | --- | --- | --- | --- | --- |\n| Claude Sonnet 5.5 | $2 / MTok | $2.50 / MTok | $4 / MTok | $0.10 / MTok | $10 / MTok |\n## Other rates\n| Claude Sonnet 5.5 | $1 / MTok | $2.50 / MTok | $4 / MTok | $0.10 / MTok | $5 / MTok |"
}
fn snapshot(entry: &Value) -> &Value {
    &entry["models"][0]["price_snapshot"]
}

#[test]
fn exact_models_and_standard_tiers_are_parsed_without_batch_confusion() {
    let gemini =
        pricing::parse_document("gemini", &google("$2.25", "$40.00"), "2026-11-01T00:00:00Z")
            .unwrap();
    assert_eq!(snapshot(&gemini)["input_microusd_per_million"], 2_250_000);
    assert_eq!(
        snapshot(&gemini)["image_output_microusd_per_million"],
        40_000_000
    );
    assert_eq!(
        gemini["models"][0]["examples"][0]["output_microusd"],
        44_800
    );
    let ai = pricing::parse_document("openai", &openai(), "2026-11-01").unwrap();
    assert_eq!(ai["models"].as_array().unwrap().len(), 2);
    assert_eq!(
        snapshot(&ai)["image_output_microusd_per_million"],
        30_000_000
    );
    assert_eq!(snapshot(&ai)["image_input_microusd_per_million"], 8_000_000);
    assert_eq!(ai["models"][0]["examples"][1]["output_microusd"], 13_170);
    let claude = pricing::parse_document("anthropic", anthropic(), "2026-11-01").unwrap();
    assert_eq!(
        snapshot(&claude)["text_input_microusd_per_million"],
        2_000_000
    );
    assert_eq!(
        snapshot(&claude)["text_output_microusd_per_million"],
        10_000_000
    );
    assert_eq!(claude["no_image_generation"], true);
    assert_eq!(claude["models"][0]["examples"], json!([]));
}

#[test]
fn malformed_currency_negative_overflow_missing_and_ambiguous_sources_fail_closed() {
    for amount in [
        "-$1.50",
        "$-1.50",
        "€1.50",
        "$0",
        "$1e6",
        "$18446744073709551615",
        "$1.1234567",
        "$1,50",
        "$1001",
    ] {
        assert!(
            pricing::parse_document("gemini", &google(amount, "$30.00"), "now").is_err(),
            "{amount}"
        );
    }
    let original = google("$1.50", "$30.00");
    for changed in [
        original.replace("per 1M tokens in USD", "per token in EUR"),
        original.replace("Input price", "Missing"),
        original.replace("consume 1120", "consume -1"),
        format!("{original}{original}"),
    ] {
        assert!(pricing::parse_document("gemini", &changed, "now").is_err());
    }
    for changed in [
        openai().replace("data-value='standard'", "data-value='batch'"),
        openai().replace("gpt-image-2.5-flare", "unknown-model"),
        openai().replace("$30.00", "$-30.00"),
        openai().replace("Prices per 1M tokens.", "Prices per token."),
        format!("{}{}", openai(), openai()),
    ] {
        assert!(pricing::parse_document("openai", &changed, "now").is_err());
    }
    assert!(
        pricing::parse_document(
            "anthropic",
            &anthropic().replace(" / MTok", " / token"),
            "now"
        )
        .is_err()
    );
    assert!(
        pricing::parse_document(
            "anthropic",
            &anthropic().replace("All prices are in USD.", ""),
            "now"
        )
        .is_err()
    );
    assert!(pricing::parse_document("unknown", &original, "now").is_err());
    assert!(
        pricing::parse_document(
            "anthropic",
            &format!("{}{}", anthropic(), anthropic()),
            "now"
        )
        .is_err()
    );
    assert!(pricing::parse_document("gemini", &"a".repeat(2 * 1024 * 1024 + 1), "now").is_err());
}

#[test]
fn rejected_refresh_preserves_all_prices_and_verification_date() {
    let old = pricing::bundled_catalog()["providers"][0].clone();
    let failed = pricing::apply_refresh_result(
        old.clone(),
        Err(anyhow::anyhow!("Synthetic timeout")),
        "2026-11-02",
    );
    assert_eq!(failed["models"], old["models"]);
    assert_eq!(failed["checked_at"], old["checked_at"]);
    assert_eq!(failed["status"], "error");
    assert_eq!(failed["last_attempt_at"], "2026-11-02");
    let fresh = pricing::parse_document("gemini", &google("$2", "$40"), "2026-11-03").unwrap();
    let updated = pricing::apply_refresh_result(failed, Ok(fresh), "2026-11-03");
    assert_eq!(updated["status"], "updated");
    assert!(updated.get("error").is_none());
    assert_ne!(snapshot(&updated)["version"], snapshot(&old)["version"]);
}

#[test]
fn estimates_and_usage_use_captured_rates_and_local_sources_cannot_override_them() {
    let parameters =
        json!({"provider":"gemini","model":provider::MODEL,"resolution":"1K","aspect_ratio":"1:1"});
    let catalog = pricing::bundled_catalog();
    let old = pricing::snapshot_for(&catalog, &parameters).unwrap();
    let entry = pricing::parse_document("gemini", &google("$3", "$60"), "2026-11-01").unwrap();
    let new = snapshot(&entry);
    let prior = provider::estimate_at(&parameters, "abcdefghij", 0, 1, &old).unwrap();
    let later = provider::estimate_at(&parameters, "abcdefghij", 0, 1, new).unwrap();
    assert_eq!(
        later["per_image_microusd"].as_u64().unwrap()
            - prior["per_image_microusd"].as_u64().unwrap(),
        33_615
    );
    assert_eq!(prior["price_snapshot"], old);
    let usage = json!({"total_input_tokens":10,"total_output_tokens":1120,"total_thought_tokens":0,"output_tokens_by_modality":[{"modality":"image","tokens":1120}]});
    assert_eq!(provider::usage_cost_at(&usage, &old), Some(33_615));
    assert_eq!(provider::usage_cost_at(&usage, new), Some(67_230));
    let ai_args = json!({"provider":"openai","model":provider::openai::SUNBURST,"resolution":"1K","aspect_ratio":"1:1","quality":"medium"});
    let mut ai = pricing::snapshot_for(&catalog, &ai_args).unwrap();
    ai["text_input_microusd_per_million"] = json!(10_000_000);
    ai["image_input_microusd_per_million"] = json!(16_000_000);
    ai["image_output_microusd_per_million"] = json!(60_000_000);
    let result = provider::estimate_at(&ai_args, "abc", 1, 1, &ai).unwrap();
    assert_eq!(result["per_image_microusd"], 30 + 8192 * 16 + 439 * 60);
    assert!(
        pricing::snapshot_for(&catalog, &json!({"provider":"openai","model":"invented"})).is_err()
    );
}

#[tokio::test]
#[ignore = "Read-only network verification of public official documentation; never provider API calls"]
async fn live_public_document_refresh() {
    let catalog = pricing::refresh(&pricing::bundled_catalog(), None)
        .await
        .unwrap();
    for provider in catalog["providers"].as_array().unwrap() {
        println!(
            "{}: {} {}",
            provider["id"], provider["status"], provider["error"]
        );
        assert_eq!(provider["status"], "updated");
    }
}
