//! Public documentation retrieval only. Never uses provider credentials or executes remote code.
use crate::provider;
use anyhow::{Context, Result, bail, ensure};
use chrono::Utc;
use scraper::{ElementRef, Html, Selector};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::time::Duration;

pub const GOOGLE_SOURCE: &str = "https://ai.google.dev/gemini-api/docs/pricing";
pub const OPENAI_SOURCE: &str = "https://developers.openai.com/api/docs/pricing";
pub const ANTHROPIC_SOURCE: &str = "https://platform.claude.com/docs/en/about-claude/pricing.md";
const ANTHROPIC_VISION: &str = "https://platform.claude.com/docs/en/build-with-claude/vision.md";
pub const OPENAI_GUIDE: &str = "https://developers.openai.com/api/docs/guides/image-generation";
const CALCULATOR_SOURCE: &str =
    "https://developers.openai.com/_astro/GptImageTokenCalculator.react.CxseQDUo.js";
const CALCULATOR_HASH: &str = "00a23a754e5b3102f35f0823c5cbaa40c0bfbdc68f99eea6ce8f40067badf8b9";
const MAX_DOCUMENT: usize = 2 * 1024 * 1024;
const SOURCES: [&str; 6] = [
    GOOGLE_SOURCE,
    OPENAI_SOURCE,
    ANTHROPIC_SOURCE,
    ANTHROPIC_VISION,
    OPENAI_GUIDE,
    CALCULATOR_SOURCE,
];

pub fn bundled_catalog() -> Value {
    let google = model_entry(
        provider::MODEL,
        "Gemini Nano Banana 2.1",
        provider::price_snapshot(),
    )
    .expect("bundled prices valid");
    let openai: Vec<Value> = provider::openai::models()
        .iter()
        .map(|model| {
            model_entry(
                model["id"].as_str().unwrap(),
                model["name"].as_str().unwrap(),
                provider::openai::price_snapshot(),
            )
            .expect("bundled prices valid")
        })
        .collect();
    json!({"currency":"USD","providers":[
        {"id":"gemini","name":"Google Gemini","capability":"image_generation","status":"bundled","checked_at":"2026-10-31","source":GOOGLE_SOURCE,"models":[google]},
        {"id":"openai","name":"OpenAI","capability":"image_generation","status":"bundled","checked_at":"2026-10-31","source":OPENAI_SOURCE,"models":openai},
        {"id":"anthropic","name":"Anthropic / Claude","capability":"prompt_assistance","no_image_generation":true,"status":"bundled","checked_at":"2026-10-31","source":ANTHROPIC_SOURCE,"capability_source":ANTHROPIC_VISION,"models":[{"id":"claude-sonnet-5-5","name":"Claude Sonnet 5.5","price_snapshot":anthropic_snapshot(),"examples":[]}],"notice":"Claude kann Bilder analysieren, aber keine Bilder erzeugen oder bearbeiten. Daher gibt es keinen Bildgenerierungspreis."}
    ]})
}

pub fn snapshot_for(catalog: &Value, parameters: &Value) -> Result<Value> {
    if parameters["provider"] == "test" {
        return Ok(provider::price_snapshot_for(parameters));
    }
    let entry = provider_entry(
        catalog,
        parameters["provider"].as_str().context("Provider fehlt")?,
    )?;
    let matches: Vec<&Value> = entry["models"]
        .as_array()
        .context("Modellpreise fehlen")?
        .iter()
        .filter(|m| m["id"] == parameters["model"])
        .collect();
    ensure!(
        matches.len() == 1,
        "Keine eindeutige Preisgrundlage für dieses Modell"
    );
    Ok(matches[0]["price_snapshot"].clone())
}
fn provider_entry<'a>(catalog: &'a Value, id: &str) -> Result<&'a Value> {
    let entries: Vec<&Value> = catalog["providers"]
        .as_array()
        .context("Preiskatalog ungültig")?
        .iter()
        .filter(|p| p["id"] == id)
        .collect();
    ensure!(
        entries.len() == 1,
        "Unbekannter oder mehrfacher Preisanbieter"
    );
    Ok(entries[0])
}
fn model_entry(id: &str, name: &str, mut price: Value) -> Result<Value> {
    price["model"] = json!(id);
    let mut examples = Vec::new();
    if id == provider::MODEL {
        for resolution in ["1K", "2K", "4K"] {
            let tokens = price["image_output_tokens"][resolution]
                .as_u64()
                .context("Bildtoken fehlen")?;
            examples.push(json!({"label":format!("{resolution} · 1:1"),"resolution":resolution,"aspect_ratio":"1:1","output_microusd":provider::cost_component(tokens,&price,"image_output_microusd_per_million")?}));
        }
    } else if id.starts_with("gpt-image-") {
        for quality in ["low", "medium", "high", "xhigh", "max"] {
            let tokens = provider::openai::output_tokens(1024, 1024, quality, &price)?;
            examples.push(json!({"label":format!("1024 × 1024 · {quality}"),"resolution":"1K","aspect_ratio":"1:1","quality":quality,"output_microusd":provider::cost_component(tokens,&price,"image_output_microusd_per_million")?}));
        }
    }
    Ok(json!({"id":id,"name":name,"price_snapshot":price,"examples":examples}))
}

/// Each provider candidate succeeds or fails as a whole. Failure never advances checked_at.
pub async fn refresh(previous: &Value, selected: Option<&str>) -> Result<Value> {
    if let Some(id) = selected {
        ensure!(
            ["gemini", "openai", "anthropic"].contains(&id),
            "Unbekannter Preisanbieter"
        );
    }
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(20))
        .connect_timeout(Duration::from_secs(7))
        .user_agent("FrameStudio/0.1 public-price-check")
        .build()?;
    let (google, openai, anthropic) = tokio::join!(
        refresh_one(&client, previous, "gemini", selected),
        refresh_one(&client, previous, "openai", selected),
        refresh_one(&client, previous, "anthropic", selected)
    );
    Ok(json!({"currency":"USD","providers":[google?,openai?,anthropic?]}))
}
async fn refresh_one(
    client: &reqwest::Client,
    previous: &Value,
    id: &str,
    selected: Option<&str>,
) -> Result<Value> {
    let old = provider_entry(previous, id)?.clone();
    if selected.is_some_and(|selected| selected != id) {
        return Ok(old);
    }
    let attempt = Utc::now().to_rfc3339();
    let candidate = retrieve(client, id, &attempt).await;
    Ok(apply_refresh_result(old, candidate, &attempt))
}
/// Public for deterministic failure/persistence tests; never accepts network destinations.
pub fn apply_refresh_result(mut old: Value, candidate: Result<Value>, attempt: &str) -> Value {
    match candidate {
        Ok(mut value) => {
            value["status"] = json!("updated");
            value["last_attempt_at"] = json!(attempt);
            value
        }
        Err(error) => {
            old["status"] = json!("error");
            old["last_attempt_at"] = json!(attempt);
            old["error"] = json!(error.to_string());
            old
        }
    }
}
async fn retrieve(client: &reqwest::Client, id: &str, checked_at: &str) -> Result<Value> {
    let source = match id {
        "gemini" => GOOGLE_SOURCE,
        "openai" => OPENAI_SOURCE,
        "anthropic" => ANTHROPIC_SOURCE,
        _ => bail!("Unbekannter Preisanbieter"),
    };
    let document = fetch(client, source).await?;
    let mut result = parse_document(id, &document, checked_at)?;
    if id == "openai" {
        let (guide, calculator) = tokio::try_join!(
            fetch(client, OPENAI_GUIDE),
            fetch(client, CALCULATOR_SOURCE)
        )?;
        verify_calculator(&guide, &calculator)?;
        result["calculator_sha256"] = json!(CALCULATOR_HASH);
    }
    if id == "anthropic" {
        let vision = fetch(client, ANTHROPIC_VISION).await?;
        verify_anthropic(&vision)?;
        result["capability_source_sha256"] = json!(hash(&vision));
    }
    Ok(result)
}
async fn fetch(client: &reqwest::Client, source: &str) -> Result<String> {
    ensure!(
        SOURCES.contains(&source),
        "Preisquelle ist nicht zugelassen"
    );
    let mut response = client.get(source).send().await.map_err(|_| {
        anyhow::anyhow!(
            "Preisquelle momentan nicht erreichbar; gespeicherte Preise bleiben erhalten"
        )
    })?;
    ensure!(
        response.status().is_success(),
        "Preisquelle lieferte HTTP {}; gespeicherte Preise bleiben erhalten",
        response.status().as_u16()
    );
    ensure!(
        response
            .content_length()
            .is_none_or(|n| n <= MAX_DOCUMENT as u64),
        "Preisdokument überschreitet Größenlimit"
    );
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| {
        anyhow::anyhow!("Preisdokument unvollständig; gespeicherte Preise bleiben erhalten")
    })? {
        ensure!(
            bytes.len() + chunk.len() <= MAX_DOCUMENT,
            "Preisdokument überschreitet Größenlimit"
        );
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes).context("Preisdokument ist kein gültiger UTF-8-Text")
}
fn verify_calculator(guide: &str, calculator: &str) -> Result<()> {
    let html = Html::parse_document(guide);
    let modules: Vec<_> = html
        .select(&selector("astro-island[component-url]")?)
        .filter(|e| {
            e.value()
                .attr("component-url")
                .is_some_and(|url| url.contains("GptImageTokenCalculator"))
        })
        .collect();
    ensure!(
        modules.len() == 1,
        "OpenAI-Rechnerstruktur geändert; Preisprüfung benötigt ein App-Update"
    );
    let module = modules[0];
    ensure!(
        module
            .value()
            .attr("component-url")
            .unwrap()
            .split('?')
            .next()
            == Some("/_astro/GptImageTokenCalculator.react.CxseQDUo.js"),
        "OpenAI-Rechner geändert; Preisprüfung benötigt ein App-Update"
    );
    ensure!(
        normalized(module).contains("GPT Image 2.5"),
        "OpenAI-Rechner-Modell nicht verifiziert"
    );
    ensure!(
        hash(calculator) == CALCULATOR_HASH,
        "OpenAI-Rechner geändert; Preisprüfung benötigt ein App-Update"
    );
    Ok(())
}

pub fn parse_document(id: &str, document: &str, checked_at: &str) -> Result<Value> {
    ensure!(
        document.len() <= MAX_DOCUMENT,
        "Preisdokument überschreitet Größenlimit"
    );
    let mut entry = provider_entry(&bundled_catalog(), id)?.clone();
    let digest = hash(document);
    let mut prices = match id {
        "gemini" => vec![parse_google(document)?],
        "openai" => parse_openai(document)?,
        "anthropic" => vec![parse_anthropic(document)?],
        _ => bail!("Unbekannter Preisanbieter"),
    };
    for price in &mut prices {
        price["version"] = json!(format!("{id}-standard-{checked_at}-{}", &digest[..12]));
        price["checked_at"] = json!(checked_at);
        price["source_sha256"] = json!(digest);
    }
    {
        let names = entry["models"].as_array().context("Modellpreise fehlen")?;
        ensure!(names.len() == prices.len(), "Unvollständige Modellpreise");
        entry["models"] = Value::Array(
            names
                .iter()
                .zip(prices)
                .map(|(model, price)| {
                    model_entry(
                        model["id"].as_str().unwrap(),
                        model["name"].as_str().unwrap(),
                        price,
                    )
                })
                .collect::<Result<_>>()?,
        );
    }
    entry["checked_at"] = json!(checked_at);
    entry["source_sha256"] = json!(digest);
    Ok(entry)
}
fn hash(value: &str) -> String {
    Sha256::digest(value.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn selector(value: &str) -> Result<Selector> {
    Selector::parse(value).map_err(|_| anyhow::anyhow!("Interner Preisparser-Selektor ungültig"))
}
fn normalized(element: ElementRef<'_>) -> String {
    element
        .text()
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
fn cells(row: ElementRef<'_>, tag: &str) -> Result<Vec<String>> {
    Ok(row.select(&selector(tag)?).map(normalized).collect())
}

fn decimal_usd(value: &str) -> Result<u64> {
    let value = value.strip_prefix('$').context("USD-Preis fehlt")?;
    let pieces: Vec<_> = value.split('.').collect();
    ensure!(
        pieces.len() <= 2
            && !pieces[0].is_empty()
            && pieces.iter().all(|p| p.bytes().all(|b| b.is_ascii_digit())),
        "Ungültiger USD-Preis"
    );
    let whole = pieces[0]
        .parse::<u64>()
        .context("Preis außerhalb des Wertebereichs")?;
    let fraction = pieces.get(1).copied().unwrap_or("");
    ensure!(
        fraction.len() <= 6,
        "Preis enthält nicht unterstützte Untereinheiten"
    );
    let fraction = format!("{fraction:0<6}").parse::<u64>()?;
    let amount = whole
        .checked_mul(1_000_000)
        .and_then(|n| n.checked_add(fraction))
        .context("Preisüberlauf")?;
    ensure!(
        (1..=1_000_000_000).contains(&amount),
        "Preis außerhalb plausibler Grenzen; manuelle Prüfung erforderlich"
    );
    Ok(amount)
}
fn labelled_price(value: &str, label: &str) -> Result<u64> {
    let pieces: Vec<&str> = value.split(label).collect();
    ensure!(pieces.len() == 2, "Preiszuordnung nicht eindeutig");
    let before = pieces[0].trim();
    let token = before.split_whitespace().last().context("Preis fehlt")?;
    decimal_usd(token)
}
fn unique_row(table: ElementRef<'_>, label: &str) -> Result<Vec<String>> {
    let mut found = Vec::new();
    for row in table.select(&selector("tbody tr")?) {
        let row = cells(row, "td")?;
        if row.first().is_some_and(|cell| cell == label) {
            found.push(row);
        }
    }
    ensure!(found.len() == 1, "Preiszeile fehlt oder ist mehrdeutig");
    Ok(found.remove(0))
}
fn parse_google(document: &str) -> Result<Value> {
    let html = Html::parse_document(document);
    let headings: Vec<_> = html
        .select(&selector("h2")?)
        .filter(|h| h.value().attr("id") == Some(provider::MODEL))
        .collect();
    ensure!(
        headings.len() == 1,
        "Google-Modellabschnitt nicht eindeutig"
    );
    let mut inside = false;
    let mut standard = false;
    let mut tables = Vec::new();
    let mut paragraphs = Vec::new();
    for element in html.select(&selector("h2,h3,table,p")?) {
        if element.value().name() == "h2" {
            if inside {
                break;
            }
            inside = element.value().attr("id") == Some(provider::MODEL);
        } else if inside {
            match element.value().name() {
                "h3" => standard = normalized(element) == "Standard",
                "table" if standard => tables.push(element),
                "p" => paragraphs.push(normalized(element)),
                _ => (),
            }
        }
    }
    ensure!(
        tables.len() == 1,
        "Google-Standardpreistabelle nicht eindeutig"
    );
    let table = tables[0];
    let header: Vec<_> = table
        .select(&selector("thead th")?)
        .map(normalized)
        .collect();
    ensure!(
        header == ["", "Free Tier", "Paid Tier, per 1M tokens in USD"],
        "Google-Preiseinheit oder Tabellenschema geändert"
    );
    let input = unique_row(table, "Input price")?;
    let output = unique_row(table, "Output price")?;
    ensure!(
        input.len() == 3 && output.len() == 3,
        "Google-Preisspalten geändert"
    );
    let mut price = provider::price_snapshot();
    price["input_microusd_per_million"] = json!(labelled_price(&input[2], "(text/image/video)")?);
    price["text_output_microusd_per_million"] =
        json!(labelled_price(&output[2], "(text and thinking)")?);
    price["image_output_microusd_per_million"] = json!(labelled_price(&output[2], "(images)")?);
    let footnote = paragraphs.join(" ");
    for resolution in ["1K", "2K", "4K"] {
        price["image_output_tokens"][resolution] = json!(google_tokens(&footnote, resolution)?);
    }
    Ok(price)
}
fn google_tokens(text: &str, resolution: &str) -> Result<u64> {
    let marker = format!("Output images at {resolution} ");
    let pieces: Vec<_> = text.split(&marker).collect();
    ensure!(
        pieces.len() == 2,
        "Google-Bildtokenangabe fehlt oder ist mehrdeutig"
    );
    let sentence = pieces[1]
        .split("tokens")
        .next()
        .context("Google-Bildtokenangabe fehlt")?;
    let tail = sentence
        .split(" consume ")
        .nth(1)
        .context("Google-Bildtokenformat geändert")?
        .trim();
    let count = tail.parse::<u64>().context("Ungültige Bildtokenzahl")?;
    ensure!(
        (1..=100_000).contains(&count),
        "Bildtokenzahl außerhalb plausibler Grenzen"
    );
    Ok(count)
}
fn parse_openai(document: &str) -> Result<Vec<Value>> {
    let html = Html::parse_document(document);
    let headings: Vec<_> = html.select(&selector("#image-generation")?).collect();
    ensure!(
        headings.len() == 1,
        "OpenAI-Bildpreisabschnitt nicht eindeutig"
    );
    let section = headings[0]
        .ancestors()
        .filter_map(ElementRef::wrap)
        .find(|e| {
            e.value().has_class(
                "pricing-multimodal-subsection",
                scraper::CaseSensitivity::CaseSensitive,
            )
        })
        .context("OpenAI-Abschnittsstruktur geändert")?;
    let units: Vec<_> = section
        .select(&selector(".pricing-switcher-meta")?)
        .map(normalized)
        .collect();
    ensure!(
        units == ["Prices per 1M tokens."],
        "OpenAI-Preiseinheit geändert"
    );
    let tables: Vec<_> = section
        .select(&selector(
            "[data-content-switcher-pane][data-value='standard'] table",
        )?)
        .collect();
    ensure!(
        tables.len() == 1,
        "OpenAI-Standardpreistabelle nicht eindeutig"
    );
    let table = tables[0];
    let headings: Vec<_> = table
        .select(&selector("thead th")?)
        .map(normalized)
        .collect();
    ensure!(
        headings == ["Model", "Modality", "Input", "Output", "Cached input"],
        "OpenAI-Preisspalten geändert"
    );
    let rows: Vec<_> = table
        .select(&selector("tbody tr")?)
        .map(|row| cells(row, "td"))
        .collect::<Result<_>>()?;
    ["gpt-image-2.5-sunburst", "gpt-image-2.5-flare"]
        .iter()
        .map(|name| openai_model_price(&rows, name))
        .collect()
}
fn openai_model_price(rows: &[Vec<String>], name: &str) -> Result<Value> {
    let indexes: Vec<_> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| row.first().is_some_and(|v| v == name))
        .map(|(i, _)| i)
        .collect();
    ensure!(
        indexes.len() == 1,
        "OpenAI-Modellpreis fehlt oder ist mehrdeutig"
    );
    let image = &rows[indexes[0]];
    let text = rows
        .get(indexes[0] + 1)
        .context("OpenAI-Texteingabepreis fehlt")?;
    ensure!(
        image.len() == 5
            && text.len() == 4
            && image[1] == "Image"
            && text[0] == "Text"
            && text[2] == "-",
        "OpenAI-Modalitäten oder Abrechnungsart geändert"
    );
    let mut price = provider::openai::price_snapshot();
    price["image_input_microusd_per_million"] = json!(decimal_usd(&image[2])?);
    price["image_output_microusd_per_million"] = json!(decimal_usd(&image[3])?);
    price["text_input_microusd_per_million"] = json!(decimal_usd(&text[1])?);
    Ok(price)
}
fn verify_anthropic(document: &str) -> Result<()> {
    let normalized = document.split_whitespace().collect::<Vec<_>>().join(" ");
    let heading = "Can Claude generate or edit images?";
    let parts: Vec<_> = normalized.split(heading).collect();
    ensure!(
        parts.len() == 2,
        "Anthropic-Fähigkeitsangabe nicht eindeutig; manuelle Prüfung erforderlich"
    );
    let answer = parts[1].chars().take(500).collect::<String>();
    ensure!(
        answer.contains("No, Claude is an image understanding model only.")
            && answer.contains("it cannot generate, produce, edit, manipulate, or create images."),
        "Anthropic-Fähigkeitsangabe geändert; manuelle Prüfung erforderlich"
    );
    Ok(())
}

pub fn anthropic_snapshot() -> Value {
    json!({"provider":"anthropic","model":"claude-sonnet-5-5","version":"anthropic-standard-2026-10-31-v1","currency":"USD","checked_at":"2026-10-31","source":ANTHROPIC_SOURCE,"text_input_microusd_per_million":2_000_000,"text_output_microusd_per_million":10_000_000})
}
fn parse_anthropic(document: &str) -> Result<Value> {
    let sections: Vec<_> = document.split("## Model pricing").collect();
    ensure!(
        sections.len() == 2,
        "Anthropic-Modellpreise fehlen oder sind mehrdeutig"
    );
    let section = sections[1].split("\n## ").next().unwrap();
    ensure!(
        document.contains("All prices are in USD."),
        "Anthropic-Abrechnungswährung ungeprüft"
    );
    let rows: Vec<Vec<&str>> = section
        .lines()
        .filter(|line| line.trim_start().starts_with('|'))
        .map(|line| {
            line.trim()
                .trim_matches('|')
                .split('|')
                .map(str::trim)
                .collect()
        })
        .collect();
    ensure!(
        rows.first().is_some_and(|r| r
            == &[
                "Model",
                "Base input tokens",
                "5m cache writes",
                "1h cache writes",
                "Cache hits and refreshes",
                "Output tokens"
            ]),
        "Anthropic-Preistabelle geändert"
    );
    let matches: Vec<_> = rows
        .iter()
        .filter(|row| row.first() == Some(&"Claude Sonnet 5.5"))
        .collect();
    ensure!(
        matches.len() == 1 && matches[0].len() == 6,
        "Anthropic-Modellpreis nicht eindeutig"
    );
    let row = matches[0];
    let mut price = anthropic_snapshot();
    price["text_input_microusd_per_million"] = json!(decimal_usd(
        row[1]
            .strip_suffix(" / MTok")
            .context("Anthropic-Preiseinheit geändert")?
    )?);
    price["text_output_microusd_per_million"] = json!(decimal_usd(
        row[5]
            .strip_suffix(" / MTok")
            .context("Anthropic-Preiseinheit geändert")?
    )?);
    Ok(price)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changed_calculator_or_capability_never_silently_changes_support() {
        let guide = "<astro-island component-url='/_astro/GptImageTokenCalculator.react.CxseQDUo.js'>GPT Image 2.5</astro-island>";
        assert!(verify_calculator(guide, "changed code").is_err());
        assert!(
            verify_calculator(&guide.replace("CxseQDUo", "different"), "changed code").is_err()
        );
        assert!(verify_calculator(&format!("{guide}{guide}"), "changed code").is_err());
        let vision = "Can Claude generate or edit images? No, Claude is an image understanding model only. While it can interpret and analyze images, it cannot generate, produce, edit, manipulate, or create images.";
        assert!(verify_anthropic(vision).is_ok());
        assert!(verify_anthropic(&vision.replace("cannot", "can")).is_err());
        assert!(verify_anthropic(&format!("{vision}{vision}")).is_err());
    }

    #[tokio::test]
    async fn unapproved_sources_and_provider_names_are_rejected_before_network() {
        let client = reqwest::Client::new();
        for source in [
            "http://127.0.0.1/",
            "https://example.com/",
            "file:///tmp/prices",
            "https://ai.google.dev/gemini-api/docs/pricing?url=other",
        ] {
            assert!(fetch(&client, source).await.is_err());
        }
        assert!(
            refresh(&bundled_catalog(), Some("https://example.com"))
                .await
                .is_err()
        );
    }
}
