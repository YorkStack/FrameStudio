//! Bounded provider adapter. No retries and no user-selected network destination.
use anyhow::{Context, Result, anyhow, bail};
use base64::{Engine, engine::general_purpose::STANDARD};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{io::Cursor, time::Duration};

mod errors;
pub mod openai;

pub const MODEL: &str = "gemini-nano-banana-2.1";
pub const PRICE_VERSION: &str = "google-standard-2026-10-31-v1";
pub const MAX_IMAGE_BYTES: usize = 20 * 1024 * 1024;
pub const MAX_PIXELS: u64 = 40_000_000;

pub fn models() -> Value {
    let mut result = json!([
      {"id":MODEL,"name":"Gemini Nano Banana 2.1","provider":"gemini","aspect_ratios":["1:1","2:3","3:2","3:4","4:3","4:5","5:4","9:16","16:9","21:9"],"resolutions":["1K","2K","4K"],"supports_references":true,"max_references":8,"supports_seed":false,"supports_negative_prompt":false,"supports_transparency":false},
      {"id":"deterministic-v1","name":"Testprovider · deterministisches Prüfbild","provider":"test","aspect_ratios":["1:1","2:3","3:2","3:4","4:3","4:5","5:4","9:16","16:9","21:9"],"resolutions":["1K"],"supports_references":true,"max_references":8,"supports_seed":false,"supports_negative_prompt":false,"supports_transparency":false}
    ]);
    result.as_array_mut().unwrap().extend(openai::models());
    result
}

pub fn validate(args: &Value) -> Result<()> {
    let all = models();
    let model = all
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == args["model"] && m["provider"] == args["provider"])
        .ok_or_else(|| anyhow!("Unbekannter Provider oder nicht unterstütztes Modell"))?;
    for (arg, list) in [
        ("aspect_ratio", "aspect_ratios"),
        ("resolution", "resolutions"),
    ] {
        if !model[list].as_array().unwrap().contains(&args[arg]) {
            bail!("Nicht unterstützter Modellparameter: {arg}");
        }
    }
    if let Some(quality) = args.get("quality")
        && !model["qualities"]
            .as_array()
            .is_some_and(|values| values.contains(quality))
    {
        bail!("Nicht unterstützte Bildqualität");
    }
    for field in [
        "seed",
        "negative_prompt",
        "tools",
        "temperature",
        "transparent",
        "endpoint",
        "url",
    ] {
        if args.get(field).is_some_and(|v| !v.is_null()) {
            bail!("Nicht unterstützter Parameter: {field}");
        }
    }
    Ok(())
}

pub fn price_snapshot() -> Value {
    json!({"version":PRICE_VERSION,"currency":"USD","checked_at":"2026-10-31","source":"https://ai.google.dev/gemini-api/docs/pricing","input_microusd_per_million":1500000,"text_output_microusd_per_million":7500000,"image_output_microusd_per_million":30000000,"image_output_tokens":{"1K":1120,"2K":1680,"4K":3780},"estimate_thinking_tokens":2048,"estimate_reference_tokens":4096})
}

pub fn price_snapshot_for(parameters: &Value) -> Value {
    if parameters["provider"] == "openai" {
        openai::price_snapshot()
    } else {
        price_snapshot()
    }
}

pub fn estimate(args: &Value, prompt: &str, references: usize, count: u64) -> Result<Value> {
    estimate_at(args, prompt, references, count, &price_snapshot_for(args))
}

pub fn estimate_at(
    args: &Value,
    prompt: &str,
    references: usize,
    count: u64,
    price: &Value,
) -> Result<Value> {
    validate(args)?;
    if args["provider"] == "openai" {
        return openai::estimate_at(args, prompt, references, count, price);
    }
    let output_tokens = price["image_output_tokens"][args["resolution"].as_str().unwrap_or("1K")]
        .as_u64()
        .context("Bildtoken-Preisgrundlage fehlt")?;
    let input_tokens = (prompt.len() as u64)
        .checked_add(
            (references as u64)
                .checked_mul(
                    price["estimate_reference_tokens"]
                        .as_u64()
                        .context("Referenzschätzung fehlt")?,
                )
                .context("Kostenüberlauf")?,
        )
        .context("Kostenüberlauf")?;
    let per = if args["provider"] == "test" {
        0
    } else {
        let thought_cost = cost_component(
            price["estimate_thinking_tokens"]
                .as_u64()
                .context("Denkschätzung fehlt")?,
            price,
            "text_output_microusd_per_million",
        )?;
        cost_component(input_tokens, price, "input_microusd_per_million")?
            .checked_add(cost_component(
                output_tokens,
                price,
                "image_output_microusd_per_million",
            )?)
            .and_then(|value| value.checked_add(thought_cost))
            .context("Kostenüberlauf oder unvollständige Preisgrundlage")?
    };
    Ok(
        json!({"per_image_microusd":per,"total_microusd":per.checked_mul(count).context("Kostenüberlauf")?,"currency":"USD","price_version":price["version"],"price_snapshot":price,"warnings":["Schätzung mit Eingabe- und Denkbudget; tatsächliche Nutzung kann abweichen. Lokale Grenzen sind keine globale Abrechnungssperre."]}),
    )
}

pub(crate) fn cost_component(tokens: u64, price: &Value, key: &str) -> Result<u64> {
    let rate = price[key].as_u64().context("Preisgrundlage fehlt")?;
    u64::try_from((tokens as u128 * rate as u128).div_ceil(1_000_000)).context("Kostenüberlauf")
}

pub fn usage_cost(usage: &Value) -> Option<i64> {
    usage_cost_at(usage, &price_snapshot())
}
pub fn usage_cost_at(usage: &Value, price: &Value) -> Option<i64> {
    if price["provider"] == "openai" {
        return openai::usage_cost_at(usage, price);
    }
    let input = usage["total_input_tokens"].as_u64()?;
    let output = usage["output_tokens_by_modality"].as_array()?;
    let total = usage["total_output_tokens"].as_u64()?;
    let thought = usage["total_thought_tokens"].as_u64()?;
    let mut text = 0u64;
    let mut image = 0u64;
    for item in output {
        let tokens = item["tokens"].as_u64()?;
        match item["modality"].as_str()? {
            "text" => text = text.checked_add(tokens)?,
            "image" => image = image.checked_add(tokens)?,
            _ => return None,
        }
    }
    if text.checked_add(image)? != total {
        return None;
    }
    let input_rate = price["input_microusd_per_million"].as_u64()? as u128;
    let text_rate = price["text_output_microusd_per_million"].as_u64()? as u128;
    let image_rate = price["image_output_microusd_per_million"].as_u64()? as u128;
    let amount = (input as u128)
        .checked_mul(input_rate)?
        .checked_add((text.checked_add(thought)? as u128).checked_mul(text_rate)?)?
        .checked_add((image as u128).checked_mul(image_rate)?)?;
    i64::try_from(amount.div_ceil(1_000_000)).ok()
}

pub fn validate_image(bytes: &[u8]) -> Result<(DynamicImage, ImageFormat)> {
    if bytes.is_empty() || bytes.len() > MAX_IMAGE_BYTES {
        bail!("Bild muss zwischen 1 Byte und 20 MiB groß sein");
    }
    let format =
        image::guess_format(bytes).map_err(|_| anyhow!("Kein unterstütztes Bildformat"))?;
    if !matches!(
        format,
        ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP | ImageFormat::Tiff
    ) {
        bail!("Nur PNG, JPEG, WebP und TIFF sind erlaubt");
    }
    let dims = image::ImageReader::with_format(Cursor::new(bytes), format).into_dimensions()?;
    if u64::from(dims.0) * u64::from(dims.1) > MAX_PIXELS || dims.0 == 0 || dims.1 == 0 {
        bail!("Bild überschreitet 40 Megapixel");
    }
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    Ok((
        reader
            .decode()
            .map_err(|_| anyhow!("Bilddaten unvollständig oder beschädigt"))?,
        format,
    ))
}

pub struct Outcome {
    pub images: Vec<Vec<u8>>,
    pub usage: Value,
    pub cost: Option<i64>,
    pub error: Option<String>,
    pub uncertain: bool,
    pub provider_id: Option<String>,
}

impl Outcome {
    fn not_sent(message: &str) -> Self {
        let mut outcome = Self::failure(message, false);
        outcome.cost = Some(0);
        outcome.usage = json!({"not_sent":true});
        outcome
    }
    fn failure(message: &str, uncertain: bool) -> Self {
        Self {
            images: vec![],
            usage: Value::Null,
            cost: None,
            error: Some(message.into()),
            uncertain,
            provider_id: None,
        }
    }
}

pub fn request_body(snapshot: &Value, references: &[Value]) -> Value {
    let mut input = vec![json!({"type":"text","text":snapshot["prompt"]})];
    for reference in references {
        input.push(
            json!({"type":"image","mime_type":reference["mime"],"data":reference["data_base64"]}),
        );
    }
    // Do not send `delivery`: the deployed image endpoint rejects even "inline"
    // with HTTP 400 (observed 2026-10-31). The documented image examples omit it.
    // We still accept only embedded bytes and never fetch provider-returned URLs.
    json!({"model":MODEL,"input":input,"store":false,"response_format":{"type":"image","mime_type":"image/jpeg","aspect_ratio":snapshot["parameters"]["aspect_ratio"],"image_size":snapshot["parameters"]["resolution"]}})
}

pub fn parse_response(response: &Value) -> Outcome {
    parse_response_at(response, &price_snapshot())
}
fn parse_response_at(response: &Value, price: &Value) -> Outcome {
    let usage = response.get("usage").cloned().unwrap_or(Value::Null);
    let mut result = Outcome {
        images: vec![],
        cost: usage_cost_at(&usage, price),
        usage,
        error: None,
        uncertain: false,
        provider_id: response["id"].as_str().map(str::to_owned),
    };
    let mut blocks = Vec::new();
    if let Some(output) = response.get("output_image") {
        blocks.push(output);
    }
    if let Some(steps) = response["steps"].as_array() {
        for step in steps.iter().filter(|s| s["type"] == "model_output") {
            if let Some(content) = step["content"].as_array() {
                blocks.extend(content.iter().filter(|v| v["type"] == "image"));
            }
        }
    }
    for block in blocks {
        let Some(data) = block["data"].as_str() else {
            continue;
        };
        if data.len() > MAX_IMAGE_BYTES * 4 / 3 + 8 {
            result.error = Some("Providerbild überschreitet das Größenlimit".into());
            break;
        }
        let Ok(bytes) = STANDARD.decode(data) else {
            result.error = Some("Provider lieferte ungültige Base64-Bilddaten".into());
            break;
        };
        if validate_image(&bytes).is_err() {
            result.error = Some("Provider lieferte keine gültigen Bilddaten".into());
            break;
        }
        result.images.push(bytes);
    }
    if result.images.is_empty() && result.error.is_none() {
        result.error = Some("Providerantwort enthält kein Bild (mögliche Sicherheitsablehnung). Kein automatischer Wiederholungsversuch.".into());
    }
    if response["status"] != "completed" {
        result.error = Some("Providerauftrag nicht abgeschlossen; Ergebnis unklar. Nicht automatisch erneut senden.".into());
        result.uncertain = true;
    }
    result
}

pub async fn generate(snapshot: &Value, references: &[Value], session_required: bool) -> Outcome {
    if validate(&snapshot["parameters"]).is_err() {
        return Outcome::not_sent("Ungültige gespeicherte Providerparameter");
    }
    if snapshot["parameters"]["provider"] == "test" {
        return match test_image(snapshot) {
            Ok(bytes) => Outcome {
                images: vec![bytes],
                usage: json!({"test":true}),
                cost: Some(0),
                error: None,
                uncertain: false,
                provider_id: None,
            },
            Err(_) => Outcome::not_sent("Lokales Prüfbild konnte nicht erzeugt werden"),
        };
    }
    let provider = snapshot["parameters"]["provider"].as_str().unwrap_or("");
    let key = match crate::secrets::provider_key_for_job(provider, session_required) {
        Ok(key) => key,
        Err(error) => {
            return Outcome::not_sent(&format!("Kein Provideraufruf gesendet: {error}"));
        }
    };
    let client = match reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(180))
        .connect_timeout(Duration::from_secs(15))
        .build()
    {
        Ok(client) => client,
        Err(_) => return Outcome::not_sent("TLS-Client konnte nicht erstellt werden"),
    };
    let request = if provider == "openai" {
        match openai::authenticated_request(&client, key.as_str(), snapshot, references) {
            Ok(request) => request,
            Err(_) => {
                return Outcome::not_sent(
                    "Ungültiger OpenAI-Prompt oder nicht unterstützte Referenzdaten; nicht gesendet",
                );
            }
        }
    } else {
        client
            .post("https://generativelanguage.googleapis.com/v1beta/interactions")
            .header("x-goog-api-key", key.as_str())
            .json(&request_body(snapshot, references))
    };
    // Only the trusted core sees these values. Redact them before any diagnostic
    // enters a job record, the GUI, REST or MCP. Never log the response body.
    let mut private = vec![key.as_str(), snapshot["prompt"].as_str().unwrap_or("")];
    private.extend(references.iter().filter_map(|r| r["data_base64"].as_str()));
    receive_response_private(
        request.send().await,
        provider,
        &snapshot["price_snapshot"],
        &private,
    )
    .await
}

#[cfg(test)]
async fn receive_response(
    response: std::result::Result<reqwest::Response, reqwest::Error>,
    provider: &str,
    price: &Value,
) -> Outcome {
    receive_response_private(response, provider, price, &[]).await
}

async fn receive_response_private(
    response: std::result::Result<reqwest::Response, reqwest::Error>,
    provider: &str,
    price: &Value,
    private: &[&str],
) -> Outcome {
    let mut response = match response {
        Ok(r) => r,
        Err(_) => {
            return Outcome::failure(
                "Netzwerkfehler oder Timeout: Ergebnis unklar. Kein automatischer Wiederholungsversuch.",
                true,
            );
        }
    };
    let code = response.status().as_u16();
    let request_id = response
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .filter(|v| v.len() <= 256)
        .map(str::to_owned);
    let mut body = Vec::new();
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) if body.len() + chunk.len() <= 32 * 1024 * 1024 => {
                body.extend_from_slice(&chunk)
            }
            Ok(None) => break,
            _ => {
                return Outcome::failure(
                    "Unvollständige oder übergroße Providerantwort; Ergebnis unklar",
                    true,
                );
            }
        }
    }
    if !(200..300).contains(&code) {
        let value = serde_json::from_slice::<Value>(&body).unwrap_or(Value::Null);
        let reason = errors::diagnostic(provider, code, &value, private);
        let mut outcome = Outcome::failure(&reason, code >= 500);
        outcome.usage = value["usage"].clone();
        outcome.cost = usage_cost_at(&outcome.usage, price);
        outcome.provider_id = request_id;
        return outcome;
    }
    match serde_json::from_slice::<Value>(&body) {
        Ok(value) => {
            let mut outcome = if provider == "openai" {
                openai::parse_response_at(&value, price)
            } else {
                parse_response_at(&value, price)
            };
            if outcome.provider_id.is_none() {
                outcome.provider_id = request_id;
            }
            outcome
        }
        Err(_) => Outcome::failure("Unvollständige Providerantwort; Ergebnis unklar", true),
    }
}

fn test_image(snapshot: &Value) -> Result<Vec<u8>> {
    let hash = Sha256::digest(snapshot["prompt"].as_str().unwrap_or("test").as_bytes());
    let ratio = snapshot["parameters"]["aspect_ratio"]
        .as_str()
        .unwrap_or("1:1");
    let (a, b) = ratio.split_once(':').unwrap_or(("1", "1"));
    let (a, b): (u32, u32) = (a.parse()?, b.parse()?);
    let (w, h) = if a >= b {
        (768, 768 * b / a)
    } else {
        (768 * a / b, 768)
    };
    let mut image = RgbaImage::new(w, h);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let border = x < 12 || y < 12 || x + 12 >= w || y + 12 >= h;
        let checker = (x / 48 + y / 48) % 2 == 0;
        let disk = ((x as f64 - w as f64 / 2.).powi(2) + (y as f64 - h as f64 / 2.).powi(2))
            < (w.min(h) as f64 * 0.28).powi(2);
        let rgb = if border {
            [29, 35, 37]
        } else if disk {
            [hash[0] / 2 + 60, hash[1] / 2 + 50, hash[2] / 2 + 70]
        } else if checker {
            [232, 226, 210]
        } else {
            [245, 241, 231]
        };
        *pixel = Rgba([rgb[0], rgb[1], rgb[2], 255]);
    }
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image).write_to(&mut bytes, ImageFormat::Png)?;
    Ok(bytes.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_usage_is_unknown() {
        assert_eq!(usage_cost(&json!({})), None);
    }
    #[test]
    fn modality_usage_calculation() {
        assert_eq!(
            usage_cost(
                &json!({"total_input_tokens":100,"total_output_tokens":1124,"total_thought_tokens":2,"output_tokens_by_modality":[{"modality":"image","tokens":1120},{"modality":"text","tokens":4}]})
            ),
            Some(33795)
        );
    }
    #[test]
    fn no_image_response_is_failure() {
        let result = parse_response(
            &json!({"status":"completed","steps":[{"type":"model_output","content":[{"type":"text","text":"no"}]}]}),
        );
        assert!(result.error.is_some());
        assert_eq!(result.cost, None);
    }
    #[test]
    fn invalid_image_rejected() {
        let result = parse_response(
            &json!({"status":"completed","output_image":{"data":"bm90IGFuIGltYWdl"}}),
        );
        assert!(result.error.is_some());
    }
    #[test]
    fn deterministic_test_image() {
        let snapshot = json!({"prompt":"Synthetic test","parameters":{"aspect_ratio":"1:1"}});
        assert_eq!(
            test_image(&snapshot).unwrap(),
            test_image(&snapshot).unwrap()
        );
    }
    #[test]
    fn unsupported_seed_rejected() {
        assert!(validate(&json!({"provider":"test","model":"deterministic-v1","resolution":"1K","aspect_ratio":"1:1","seed":12})).is_err());
    }
}

#[cfg(test)]
mod historical_price_tests {
    use super::*;
    #[test]
    fn historical_price_snapshot_controls_usage_cost() {
        let usage = json!({"total_input_tokens":100,"total_output_tokens":1120,"total_thought_tokens":0,"output_tokens_by_modality":[{"modality":"image","tokens":1120}]});
        let mut price = price_snapshot();
        price["image_output_microusd_per_million"] = json!(60_000_000);
        assert_eq!(usage_cost_at(&usage, &price), Some(67350));
        assert_eq!(usage_cost_at(&usage, &json!({})), None);
    }
}

#[cfg(test)]
mod http_contract_tests {
    use super::*;
    fn response(code: u16, body: Vec<u8>) -> reqwest::Response {
        axum::http::Response::builder()
            .status(code)
            .header("x-request-id", "synthetic-request-id")
            .body(body)
            .unwrap()
            .into()
    }
    #[tokio::test]
    async fn http_errors_preserve_usage_without_exposing_raw_provider_text() {
        let body = json!({"error":{"message":"sensitive synthetic server details"},"usage":{"input_tokens":3,"input_tokens_details":{"text_tokens":3,"image_tokens":0},"output_tokens":2,"total_tokens":5}});
        for code in [400, 401, 403, 429, 500, 503] {
            let result = receive_response(
                Ok(response(code, serde_json::to_vec(&body).unwrap())),
                "openai",
                &openai::price_snapshot(),
            )
            .await;
            assert_eq!(result.cost, Some(75));
            assert_eq!(result.uncertain, code >= 500);
            assert_eq!(result.provider_id.as_deref(), Some("synthetic-request-id"));
            assert!(!result.error.unwrap().contains("sensitive"));
        }
        let without_usage = receive_response(
            Ok(response(429, b"{}".to_vec())),
            "openai",
            &openai::price_snapshot(),
        )
        .await;
        assert_eq!(without_usage.cost, None);
    }
    #[tokio::test]
    async fn openai_structured_rejections_keep_usage_and_request_id() {
        for (status, code, expected) in [
            (401, "invalid_api_key", "widerrufen"),
            (429, "credit_balance_exhausted", "aufgebraucht"),
            (400, "moderation_blocked", "Sicherheitsprüfung"),
            (503, "server_is_overloaded", "Ergebnis unklar"),
        ] {
            for usage in [
                Value::Null,
                json!({"input_tokens":3,"input_tokens_details":{"text_tokens":3,"image_tokens":0},"output_tokens":2,"total_tokens":5}),
            ] {
                let body = json!({"error":{"code":code,"message":"synthetic-private-prompt"},"usage":usage});
                let result = receive_response(
                    Ok(response(status, serde_json::to_vec(&body).unwrap())),
                    "openai",
                    &openai::price_snapshot(),
                )
                .await;
                assert!(result.images.is_empty());
                assert_eq!(result.uncertain, status >= 500);
                assert_eq!(result.cost, if usage.is_null() { None } else { Some(75) });
                assert_eq!(result.provider_id.as_deref(), Some("synthetic-request-id"));
                let message = result.error.unwrap();
                assert!(message.contains(expected));
                assert!(message.contains(code));
                assert!(!message.contains("synthetic-private"));
            }
        }
    }
    #[tokio::test]
    async fn incomplete_large_and_transport_failures_are_uncertain_without_retry() {
        for body in [b"{unfinished".to_vec(), vec![b'x'; 32 * 1024 * 1024 + 1]] {
            let result =
                receive_response(Ok(response(200, body)), "openai", &openai::price_snapshot())
                    .await;
            assert!(result.uncertain);
            assert!(result.error.is_some());
            assert!(result.images.is_empty());
            assert_eq!(result.cost, None);
        }
        // A locally produced request-builder error: no server and no network request.
        let error = reqwest::Client::new()
            .get("not a valid URL")
            .build()
            .unwrap_err();
        let result = receive_response(Err(error), "openai", &openai::price_snapshot()).await;
        assert!(result.uncertain);
        assert_eq!(result.cost, None);
    }
}
