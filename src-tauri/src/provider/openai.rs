//! OpenAI Images API only: fixed endpoints, inline references, no URL fetching.
use super::{MAX_IMAGE_BYTES, Outcome, validate_image};
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use image::ImageFormat;
use serde_json::{Value, json};
use std::io::Cursor;

pub const SUNBURST: &str = "gpt-image-2.5-sunburst-2026-09-08";
pub const FLARE: &str = "gpt-image-2.5-flare-2026-09-08";
pub const PRICE_VERSION: &str = "openai-images-standard-2026-10-31-v1";
const QUALITIES: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

pub fn models() -> Vec<Value> {
    [(SUNBURST, "GPT Image 2.5 Sunburst"), (FLARE, "GPT Image 2.5 Flare")]
        .into_iter()
        .map(|(id, name)| json!({"id":id,"name":name,"provider":"openai","aspect_ratios":["1:1","3:2","2:3"],"resolutions":["1K"],"qualities":QUALITIES,"default_quality":"medium","supports_references":true,"max_references":8,"supports_seed":false,"supports_negative_prompt":false,"supports_transparency":false}))
        .collect()
}

pub fn price_snapshot() -> Value {
    json!({"provider":"openai","version":PRICE_VERSION,"currency":"USD","checked_at":"2026-10-31","source":"https://developers.openai.com/api/docs/pricing","text_input_microusd_per_million":5000000,"image_input_microusd_per_million":8000000,"image_output_microusd_per_million":30000000,"estimate_method":"official-output-calculator-and-local-input-allowance-v1","estimate_image_input_tokens_per_reference":8192,"calculator_source":"https://developers.openai.com/api/docs/guides/image-generation#cost-and-latency","output_grid_edges":{"low":16,"medium":24,"high":48,"xhigh":64,"max":96}})
}

pub fn dimensions(args: &Value) -> Result<(u64, u64)> {
    match args["aspect_ratio"].as_str() {
        Some("1:1") => Ok((1024, 1024)),
        Some("3:2") => Ok((1536, 1024)),
        Some("2:3") => Ok((1024, 1536)),
        _ => anyhow::bail!("OpenAI-Seitenverhältnis nicht unterstützt"),
    }
}

pub fn estimate(args: &Value, prompt: &str, references: usize, count: u64) -> Result<Value> {
    estimate_at(args, prompt, references, count, &price_snapshot())
}

pub fn estimate_at(
    args: &Value,
    prompt: &str,
    references: usize,
    count: u64,
    price: &Value,
) -> Result<Value> {
    let quality = args["quality"].as_str().unwrap_or("medium");
    let (width, height) = dimensions(args)?;
    let output = output_tokens(width, height, quality, price)?;
    let input_image = (references as u64)
        .checked_mul(
            price["estimate_image_input_tokens_per_reference"]
                .as_u64()
                .context("Referenzschätzung fehlt")?,
        )
        .context("Kostenüberlauf")?;
    let text_cost = super::cost_component(
        prompt.len() as u64,
        price,
        "text_input_microusd_per_million",
    )?;
    let image_cost = super::cost_component(input_image, price, "image_input_microusd_per_million")?;
    let output_cost = super::cost_component(output, price, "image_output_microusd_per_million")?;
    let per = text_cost
        .checked_add(image_cost)
        .and_then(|v| v.checked_add(output_cost))
        .context("Kostenüberlauf")?;
    Ok(
        json!({"per_image_microusd":per,"total_microusd":per.checked_mul(count).context("Kostenüberlauf")?,"currency":"USD","price_version":price["version"],"price_snapshot":price,"estimated_output_tokens":output,"size":format!("{width}x{height}"),"quality":quality,"warnings":["Ausgabe nach offiziellem Bildtoken-Rechner; Texteingabe mit UTF-8-Byteanzahl und Referenzen pauschal mit 8192 Tokens geschätzt. Tatsächliche Nutzung kann abweichen; keine garantierte Obergrenze.","OpenAI API benötigt einen eigenen API-Zugang. Lokale Budgets sind keine globale Abrechnungssperre."]}),
    )
}

/// Integer form of the official GPT Image 2.5 calculator (checked 2026-10-31).
/// Grid rounding uses ties-to-even; money remains integer micro-USD.
pub(crate) fn output_tokens(width: u64, height: u64, quality: &str, price: &Value) -> Result<u64> {
    let edge = price["output_grid_edges"][quality]
        .as_u64()
        .context("OpenAI-Qualität nicht unterstützt")?;
    let numerator = edge * width.min(height);
    let denominator = width.max(height);
    let floor = numerator / denominator;
    let remainder = numerator % denominator;
    let rounded = floor
        + u64::from(
            remainder * 2 > denominator || (remainder * 2 == denominator && floor % 2 == 1),
        );
    Ok((edge * rounded * (2_000_000 + width * height)).div_ceil(4_000_000))
}

pub fn usage_cost_at(usage: &Value, price: &Value) -> Option<i64> {
    let input = usage["input_tokens"].as_u64()?;
    let text = usage["input_tokens_details"]["text_tokens"].as_u64()?;
    let image = usage["input_tokens_details"]["image_tokens"].as_u64()?;
    let output = usage["output_tokens"].as_u64()?;
    if text.checked_add(image)? != input
        || input.checked_add(output)? != usage["total_tokens"].as_u64()?
    {
        return None;
    }
    if let Some(details) = usage.get("output_tokens_details")
        && (details["image_tokens"].as_u64()? != output || details["text_tokens"].as_u64()? != 0)
    {
        // No published text-output rate for Images: don't guess.
        return None;
    }
    let amount = (text as u128)
        .checked_mul(price["text_input_microusd_per_million"].as_u64()? as u128)?
        .checked_add(
            (image as u128)
                .checked_mul(price["image_input_microusd_per_million"].as_u64()? as u128)?,
        )?
        .checked_add(
            (output as u128)
                .checked_mul(price["image_output_microusd_per_million"].as_u64()? as u128)?,
        )?;
    i64::try_from(amount.div_ceil(1_000_000)).ok()
}

fn reference_url(reference: &Value) -> Result<String> {
    let data = reference["data_base64"]
        .as_str()
        .context("Referenzbild fehlt")?;
    ensure!(
        data.len() <= MAX_IMAGE_BYTES * 4 / 3 + 8,
        "Referenzbild zu groß"
    );
    let bytes = STANDARD
        .decode(data)
        .context("Ungültige Referenzbilddaten")?;
    let (image, format) = validate_image(&bytes)?;
    let (bytes, mime) = if format == ImageFormat::Tiff {
        let mut output = Cursor::new(Vec::new());
        image.write_to(&mut output, ImageFormat::Png)?;
        (output.into_inner(), "image/png")
    } else {
        (bytes, format.to_mime_type())
    };
    let url = format!("data:{mime};base64,{}", STANDARD.encode(bytes));
    ensure!(
        url.len() <= 20 * 1024 * 1024,
        "OpenAI-Referenz überschreitet 20 MiB als Data-URL"
    );
    Ok(url)
}

pub fn request_body(snapshot: &Value, references: &[Value]) -> Result<Value> {
    let parameters = &snapshot["parameters"];
    super::validate(parameters)?;
    ensure!(parameters["provider"] == "openai", "OpenAI-Provider fehlt");
    ensure!(references.len() <= 8, "Maximal acht Referenzen pro Auftrag");
    let prompt = snapshot["prompt"].as_str().context("Prompt fehlt")?;
    ensure!(
        !prompt.trim().is_empty() && prompt.len() <= 32000,
        "Ungültige Promptlänge"
    );
    let (width, height) = dimensions(parameters)?;
    let mut body = json!({"model":parameters["model"],"prompt":prompt,"n":1,"size":format!("{width}x{height}"),"quality":parameters["quality"].as_str().unwrap_or("medium"),"output_format":"png","background":"opaque","moderation":"auto"});
    if !references.is_empty() {
        let images: Vec<Value> = references
            .iter()
            .map(|r| reference_url(r).map(|url| json!({"image_url":url})))
            .collect::<Result<_>>()?;
        let total: usize = images
            .iter()
            .map(|r| r["image_url"].as_str().unwrap().len())
            .sum();
        ensure!(
            total <= 32 * 1024 * 1024,
            "OpenAI-Referenzen überschreiten zusammen 32 MiB"
        );
        body["images"] = json!(images);
    }
    Ok(body)
}

pub fn endpoint(has_references: bool) -> &'static str {
    if has_references {
        "https://api.openai.com/v1/images/edits"
    } else {
        "https://api.openai.com/v1/images/generations"
    }
}

pub(crate) fn authenticated_request(
    client: &reqwest::Client,
    key: &str,
    snapshot: &Value,
    references: &[Value],
) -> Result<reqwest::RequestBuilder> {
    Ok(client
        .post(endpoint(!references.is_empty()))
        .bearer_auth(key)
        .json(&request_body(snapshot, references)?))
}

pub fn parse_response_at(response: &Value, price: &Value) -> Outcome {
    let usage = response.get("usage").cloned().unwrap_or(Value::Null);
    let mut result = Outcome {
        images: vec![],
        cost: usage_cost_at(&usage, price),
        usage,
        error: None,
        uncertain: false,
        provider_id: None,
    };
    let Some(data) = response["data"].as_array().filter(|items| items.len() == 1) else {
        result.error = Some(
            "OpenAI-Antwort enthält nicht genau ein Bild. Kein automatischer Wiederholungsversuch."
                .into(),
        );
        return result;
    };
    match decode_image(&data[0]) {
        Ok(bytes) => result.images.push(bytes),
        Err(_) => result.error = Some("OpenAI lieferte keine gültigen eingebetteten Bilddaten (mögliche Sicherheitsablehnung). Kein automatischer Wiederholungsversuch.".into()),
    }
    result
}

fn decode_image(item: &Value) -> Result<Vec<u8>> {
    let data = item["b64_json"]
        .as_str()
        .context("Eingebettetes Bild fehlt")?;
    ensure!(data.len() <= MAX_IMAGE_BYTES * 4 / 3 + 8, "Bild zu groß");
    let bytes = STANDARD.decode(data)?;
    validate_image(&bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod wire_tests {
    use super::*;
    #[test]
    fn key_is_sensitive_bearer_only_and_generation_and_edit_use_png() {
        let client = reqwest::Client::new();
        let key = "synthetic-openai-wire-key-not-real";
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(4, 4)
            .write_to(&mut png, ImageFormat::Png)
            .unwrap();
        let reference = json!({"data_base64":STANDARD.encode(png.into_inner())});
        for model in [SUNBURST, FLARE] {
            for references in [vec![], vec![reference.clone()]] {
                let snapshot = json!({"prompt":"Synthetic wire test","parameters":{"provider":"openai","model":model,"aspect_ratio":"1:1","resolution":"1K","quality":"medium"}});
                let request = authenticated_request(&client, key, &snapshot, &references)
                    .unwrap()
                    .build()
                    .unwrap();
                assert_eq!(request.method(), reqwest::Method::POST);
                assert_eq!(request.url().as_str(), endpoint(!references.is_empty()));
                assert!(request.url().query().is_none());
                let auth = &request.headers()["authorization"];
                assert!(auth.is_sensitive());
                assert!(auth.to_str().unwrap() == format!("Bearer {key}"));
                assert!(!format!("{request:?}").contains(key));
                assert!(!request.headers().contains_key("x-goog-api-key"));
                let bytes = request.body().unwrap().as_bytes().unwrap();
                assert!(!String::from_utf8_lossy(bytes).contains(key));
                let body: Value = serde_json::from_slice(bytes).unwrap();
                assert_eq!(body["output_format"], "png");
                assert!(body.get("response_format").is_none());
                assert_eq!(body.get("images").is_some(), !references.is_empty());
            }
        }
        // Only build; never execute these requests.
    }
}
