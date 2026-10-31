//! Bounded provider diagnostics. Never persist raw bodies, prompts or keys.
use serde_json::Value;

pub(super) fn diagnostic(provider: &str, code: u16, body: &Value, private: &[&str]) -> String {
    let mut summary = describe(provider, code, body);
    if provider != "gemini" {
        return summary;
    }
    // Interactions uses error.code (lower-case string), whereas other Google
    // endpoints use error.status. Both must be understood.
    let tag = body["error"]["code"].as_str().unwrap_or("");
    if matches!(
        tag,
        "invalid_argument"
            | "failed_precondition"
            | "unauthenticated"
            | "permission_denied"
            | "not_found"
            | "resource_exhausted"
            | "internal"
            | "unavailable"
            | "invalid_request_error"
    ) {
        summary.push_str(&format!(" Fehlercode: {tag}."));
    }
    if let Some(message) = body["error"]["message"].as_str() {
        let clean = redact(message, private);
        if !clean.is_empty() {
            summary.push_str("\nGoogle-Fehlermeldung (bereinigt): ");
            summary.push_str(&clean);
        }
    }
    for detail in body["error"]["details"]
        .as_array()
        .into_iter()
        .flatten()
        .take(8)
    {
        for violation in detail["fieldViolations"]
            .as_array()
            .into_iter()
            .flatten()
            .take(8)
        {
            let field = violation["field"].as_str().unwrap_or("");
            let description = violation["description"].as_str().unwrap_or("");
            summary.push_str("\nParameterhinweis: ");
            summary.push_str(&redact(&format!("{field}: {description}"), private));
        }
    }
    summary.chars().take(4000).collect()
}

fn redact(message: &str, private: &[&str]) -> String {
    let mut clean = message.to_owned();
    // Replace before truncation so even a credential at the output boundary is
    // removed entirely. Also cover JSON-escaped echoes and individual prompt lines.
    for value in private.iter().filter(|s| !s.is_empty()) {
        clean = clean.replace(value, "[entfernt]");
        if let Ok(encoded) = serde_json::to_string(value) {
            clean = clean.replace(&encoded[1..encoded.len() - 1], "[entfernt]");
        }
        for line in value.lines().filter(|line| line.len() >= 12) {
            clean = clean.replace(line, "[entfernt]");
        }
    }
    // Never retain links/query credentials, bearer values, key-shaped strings,
    // long opaque tokens/base64 or terminal/bidi control sequences.
    let normalized: String = clean
        .chars()
        .filter(|c| {
            !c.is_control() && !matches!(*c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        })
        .collect();
    let mut hide_next = false;
    let mut words = Vec::new();
    for word in normalized.split_whitespace() {
        let lower = word.to_ascii_lowercase();
        let sensitive = hide_next
            || lower.contains("http://")
            || lower.contains("https://")
            || lower.contains("aiza")
            || lower.contains("sk-")
            || lower.contains("data:")
            || lower.contains("key=")
            || lower.contains("token=")
            || word.chars().count() > 100;
        hide_next = matches!(
            lower.trim_matches(|c: char| !c.is_alphanumeric()),
            "bearer" | "authorization" | "x-goog-api-key" | "api_key" | "apikey"
        );
        words.push(if sensitive { "[entfernt]" } else { word });
    }
    let result = words.join(" ");
    if result.chars().count() > 1200 {
        format!(
            "{} … [gekürzt]",
            result.chars().take(1200).collect::<String>()
        )
    } else {
        result
    }
}

pub(super) fn describe(provider: &str, code: u16, body: &Value) -> String {
    if provider == "openai" {
        return describe_openai(code, body);
    }
    let name = if provider == "gemini" {
        "Google Gemini"
    } else {
        "OpenAI"
    };
    let status = body["error"]["status"].as_str().unwrap_or("");
    let status = match status {
        "INVALID_ARGUMENT"
        | "FAILED_PRECONDITION"
        | "UNAUTHENTICATED"
        | "PERMISSION_DENIED"
        | "NOT_FOUND"
        | "RESOURCE_EXHAUSTED"
        | "INTERNAL"
        | "UNAVAILABLE" => format!(" · {status}"),
        _ => String::new(),
    };
    let hint = known_reason(body).or_else(|| {
        (body["error"]["message"] == "Image delivery mode is not supported.").then_some(
            "Google unterstützt den angeforderten Bild-Liefermodus nicht. Der Adapter muss das optionale Feld response_format.delivery weglassen. Kein automatischer Wiederholungsversuch.")
    }).unwrap_or(match code {
        400 => "Anfrageparameter abgelehnt. Modell und Ausgabeparameter prüfen; kein automatischer Wiederholungsversuch.",
        401 | 403 => "API-Schlüssel oder Zugriffsfreigabe abgelehnt. Schlüsselbeschränkungen und Projektberechtigungen beim Anbieter prüfen.",
        402 => "Abrechnung oder verfügbares Guthaben beim Anbieter prüfen.",
        404 => "Modell oder API-Ressource nicht verfügbar. Modellfreigabe und API-Konfiguration prüfen.",
        429 => "Quote oder Rate Limit erreicht. Kontingent und Abrechnung beim Anbieter prüfen; keine automatische Wiederholung.",
        500..=599 => "Anbieterdienst meldet einen Fehler. Ergebnis unklar; nicht automatisch erneut senden.",
        _ => "Anfrage fehlgeschlagen. Kein automatischer Wiederholungsversuch.",
    });
    format!("{name}: HTTP {code}{status}. {hint}")
}

fn describe_openai(http: u16, body: &Value) -> String {
    let error = &body["error"];
    let code = error["code"].as_str().unwrap_or("");
    // Never interpolate the free-form message, parameter values or unknown codes.
    // Specific codes take precedence over the broader insufficient_quota type.
    let known = match code {
        "invalid_api_key" => Some(
            "API-Schlüssel ungültig oder widerrufen. Den OpenAI-Schlüssel und seine Projektzuordnung prüfen.",
        ),
        "credit_balance_exhausted" => Some(
            "Das API-Guthaben der OpenAI-Organisation ist aufgebraucht. Guthaben in den API-Abrechnungseinstellungen prüfen.",
        ),
        "organization_spend_limit_exceeded" => Some(
            "Das Ausgabenlimit der OpenAI-Organisation ist erreicht. API-Abrechnung und Organisationslimit prüfen.",
        ),
        "project_spend_limit_exceeded" => Some(
            "Das Ausgabenlimit des OpenAI-Projekts ist erreicht. Das zum Schlüssel gehörende Projektlimit prüfen.",
        ),
        "organization_usage_limit_exceeded" => Some(
            "Das von OpenAI zugeteilte Nutzungslimit der Organisation ist erreicht. Freigegebenes Kontingent beim Anbieter prüfen.",
        ),
        "insufficient_quota" => Some(
            "API-Kontingent oder Guthaben reicht nicht aus. Abrechnung und Limits des zugehörigen OpenAI-Projekts prüfen.",
        ),
        "slow_down" | "rate_limit_exceeded" => Some(
            "Zu viele oder zu schnell gesteigerte Anfragen. Vor einem manuellen Versuch warten; keine automatische Wiederholung.",
        ),
        "moderation_blocked" | "content_policy_violation" => Some(
            "Die Bildanfrage wurde durch eine Sicherheitsprüfung abgelehnt. Beschreibung und ausgewählte Referenzbilder prüfen und gegebenenfalls überarbeiten.",
        ),
        "model_not_found" => Some(
            "Das Bildmodell ist nicht verfügbar oder für dieses API-Projekt nicht freigegeben. Modellzugriff und Projektberechtigungen prüfen.",
        ),
        "server_is_overloaded" => Some(
            "Das Bildmodell ist vorübergehend überlastet. Ergebnis unklar; keine automatische Wiederholung.",
        ),
        _ => None,
    };
    let tag = if known.is_some() {
        format!(" · {code}")
    } else {
        String::new()
    };
    let hint = known.unwrap_or_else(|| match (http, error["type"].as_str().unwrap_or("")) {
        (429, "insufficient_quota") => "API-Kontingent oder Guthaben reicht nicht aus. Abrechnung und Limits des zugehörigen OpenAI-Projekts prüfen.",
        (400 | 422, "image_generation_user_error") => "Bildanfrage kann nicht verarbeitet werden. Beschreibung und Referenzbilder prüfen; keine automatische Wiederholung.",
        (400 | 422, _) => "Anfrageparameter abgelehnt. Modell, Bildformat, Größe, Qualität und Referenzbilder prüfen.",
        (401, _) => "Authentifizierung abgelehnt. OpenAI-API-Schlüssel, Widerruf, Projektzuordnung, Endpoint-Berechtigung und gegebenenfalls IP-Freigabe prüfen.",
        (403, _) => "Zugriff verweigert. Modellfreigabe, Projektberechtigungen und unterstützte Region bei OpenAI prüfen.",
        (404, _) => "Modell oder API-Ressource nicht verfügbar. Modellzugriff und Projektberechtigungen prüfen.",
        (413, _) => "Anfrage zu groß. Anzahl oder Dateigröße der Referenzbilder verringern.",
        (429, _) => "Rate Limit oder API-Kontingent erreicht. Kontingent und Abrechnung prüfen; keine automatische Wiederholung.",
        (500..=599, _) => "OpenAI meldet einen Serverfehler. Ergebnis unklar; keine automatische Wiederholung.",
        _ => "Anfrage fehlgeschlagen. Kein automatischer Wiederholungsversuch.",
    });
    let param = if matches!(http, 400 | 422) && known.is_none() {
        match error["param"].as_str().unwrap_or("") {
            "output_format" => " Betroffener Parameter: output_format (PNG, JPEG oder WebP).",
            "response_format" => {
                " Betroffener Parameter: response_format (bei GPT-Image nicht unterstützt)."
            }
            "size" => " Betroffener Parameter: size (Bildmaße).",
            "quality" => " Betroffener Parameter: quality (Bildqualität).",
            "model" => " Betroffener Parameter: model (Bildmodell).",
            "images" | "image" | "images[0].image_url" => " Betroffener Parameter: Referenzbilder.",
            "prompt" => " Betroffener Parameter: prompt (Beschreibung).",
            "background" => " Betroffener Parameter: background (Hintergrund).",
            _ => "",
        }
    } else {
        ""
    };
    format!("OpenAI: HTTP {http}{tag}. {hint}{param}")
}

fn known_reason(body: &Value) -> Option<&'static str> {
    for detail in body["error"]["details"].as_array().into_iter().flatten() {
        let reason = match detail["reason"].as_str().unwrap_or("") {
            "API_KEY_INVALID" | "API_KEY_EXPIRED" => Some(
                "API-Schlüssel ungültig oder abgelaufen. Einen gültigen Schlüssel des ausgewählten Anbieters verwenden.",
            ),
            "API_KEY_SERVICE_BLOCKED"
            | "API_KEY_HTTP_REFERRER_BLOCKED"
            | "API_KEY_IP_ADDRESS_BLOCKED" => Some(
                "Die Beschränkungen dieses API-Schlüssels verhindern den Desktop-Zugriff. Freigabe für die Gemini API und zulässige Herkunft beim Anbieter prüfen.",
            ),
            "SERVICE_DISABLED" => Some(
                "Die benötigte API ist im zugehörigen Google-Projekt deaktiviert. API-Freigabe im Anbieterprojekt prüfen.",
            ),
            "BILLING_DISABLED" => {
                Some("Die Abrechnung ist für das zugehörige Anbieterprojekt nicht freigeschaltet.")
            }
            _ => None,
        };
        if reason.is_some() {
            return reason;
        }
        for violation in detail["fieldViolations"].as_array().into_iter().flatten() {
            if violation["field"] == "response_format.mime_type" {
                return Some(
                    "Das angeforderte direkte Ausgabeformat wurde abgelehnt. Der Gemini-Adapter muss das dokumentierte Bildformat verwenden; lokale Exporte werden separat konvertiert.",
                );
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn observed_gemini_delivery_rejection_is_actionable() {
        let body = json!({"error":{"message":"Image delivery mode is not supported."}});
        let message = diagnostic("gemini", 400, &body, &[]);
        assert!(message.contains("response_format.delivery weglassen"));
        assert!(message.contains("Image delivery mode is not supported."));
    }
    #[test]
    fn interactions_diagnostics_explain_failure_and_redact_credentials_and_inputs() {
        let key = "synthetic-secret-key-for-this-test";
        let prompt = "Private portrait transformation\nPersonal input with \"quotes\"";
        let body = json!({"error":{"code":"invalid_argument","message":format!(
            "Unsupported image_size 1K. Key: {key}. Prompt: {prompt}. https://example.invalid/?key={key} AIzaSyntheticCredential sk-another-secret Bearer abcdef"),
            "details":[{"fieldViolations":[{"field":"response_format.image_size","description":format!("Unsupported for this model: {key}")}]}]}});
        let message = diagnostic("gemini", 400, &body, &[key, prompt]);
        assert!(message.contains("invalid_argument"));
        assert!(message.contains("Unsupported image_size 1K"));
        assert!(message.contains("response_format.image_size"));
        for value in [
            key,
            "Private portrait",
            "Personal input",
            "https://",
            "AIza",
            "sk-another",
            "abcdef",
        ] {
            assert!(!message.contains(value), "leaked {value}");
        }
    }
    #[test]
    fn diagnostics_are_bounded_and_do_not_emit_escaped_secrets_or_controls() {
        let secret = "private\"quoted\\credential";
        let escaped = serde_json::to_string(secret).unwrap();
        let body = json!({"error":{"message":format!("Invalid input: {} {} \u{202e}\u{001b}", &escaped[1..escaped.len()-1], "A".repeat(4000)), "details":vec![json!({"fieldViolations":vec![json!({"field":"input","description":"b ".repeat(2000)});20]});20]}});
        let message = diagnostic("gemini", 400, &body, &[secret]);
        assert!(!message.contains("quoted"));
        assert!(!message.contains(&"A".repeat(101)));
        assert!(!message.contains('\u{202e}'));
        assert!(!message.contains('\u{001b}'));
        assert!(message.chars().count() <= 4000);
        assert!(diagnostic("gemini", 400, &Value::Null, &[]).contains("HTTP 400"));
    }
    #[test]
    fn safe_codes_and_actionable_reasons_never_echo_unknown_server_text() {
        let secret = "synthetic-private-message-do-not-echo";
        for (reason, expected) in [
            ("API_KEY_INVALID", "ungültig"),
            ("API_KEY_SERVICE_BLOCKED", "Beschränkungen"),
            ("SERVICE_DISABLED", "deaktiviert"),
            ("BILLING_DISABLED", "Abrechnung"),
        ] {
            let body = json!({"error":{"status":"INVALID_ARGUMENT","message":secret,"details":[{"reason":reason,"metadata":{"key":secret}}]}});
            let output = describe("gemini", 400, &body);
            assert!(output.contains("HTTP 400 · INVALID_ARGUMENT"));
            assert!(output.contains(expected));
            assert!(!output.contains(secret));
        }
        let body = json!({"error":{"status":secret,"message":secret,"details":[{"reason":secret,"fieldViolations":[{"field":secret,"description":secret}]}]}});
        assert!(!describe("gemini", 400, &body).contains(secret));
        assert!(describe("gemini", 404, &body).contains("Modell"));
        assert!(describe("openai", 429, &body).contains("Rate Limit"));
    }
    #[test]
    fn invalid_image_output_has_safe_specific_guidance() {
        let body = json!({"error":{"details":[{"fieldViolations":[{"field":"response_format.mime_type","description":"private input"}]}]}});
        let message = describe("gemini", 400, &body);
        assert!(message.contains("Ausgabeformat"));
        assert!(!message.contains("private input"));
    }
}

#[cfg(test)]
mod openai_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn distinguishes_billing_limits_rate_limits_and_safety_without_echoing_secrets() {
        let private = "synthetic-private-key-or-prompt";
        for (http, code, expected) in [
            (401, "invalid_api_key", "widerrufen"),
            (429, "credit_balance_exhausted", "aufgebraucht"),
            (429, "project_spend_limit_exceeded", "Projektlimit"),
            (
                429,
                "organization_spend_limit_exceeded",
                "Organisationslimit",
            ),
            (429, "organization_usage_limit_exceeded", "Nutzungslimit"),
            (429, "slow_down", "warten"),
            (429, "rate_limit_exceeded", "warten"),
            (400, "moderation_blocked", "Sicherheitsprüfung"),
            (404, "model_not_found", "Modellzugriff"),
            (503, "server_is_overloaded", "Ergebnis unklar"),
        ] {
            let message = describe(
                "openai",
                http,
                &json!({"error":{"code":code,"type":"insufficient_quota","message":private,"param":private,"moderation_details":{"categories":[private]}}}),
            );
            assert!(message.contains(&format!("HTTP {http} · {code}")));
            assert!(message.contains(expected), "{message}");
            assert!(!message.contains(private));
        }
    }
    #[test]
    fn unknown_codes_and_parameters_are_not_echoed_and_provider_formats_are_isolated() {
        let private = "synthetic-private-value";
        let body = json!({"error":{"message":private,"code":private,"type":private,"param":private,"status":"INVALID_ARGUMENT","details":[{"reason":"API_KEY_INVALID"}]}});
        let message = describe("openai", 400, &body);
        assert!(!message.contains(private));
        assert!(!message.contains("INVALID_ARGUMENT"));
        assert!(!message.contains("ungültig"));
        assert!(message.contains("Anfrageparameter"));
        for param in [
            "output_format",
            "response_format",
            "size",
            "quality",
            "model",
            "prompt",
            "background",
        ] {
            let message = describe(
                "openai",
                400,
                &json!({"error":{"param":param,"message":private}}),
            );
            assert!(message.contains(&format!("Parameter: {param}")));
            assert!(!message.contains(private));
        }
        assert!(
            describe(
                "openai",
                429,
                &json!({"error":{"type":"insufficient_quota"}})
            )
            .contains("Guthaben")
        );
        assert!(
            describe(
                "openai",
                400,
                &json!({"error":{"type":"image_generation_user_error"}})
            )
            .contains("Referenzbilder")
        );
        assert!(describe("openai", 401, &Value::Null).contains("Authentifizierung"));
        assert!(describe("openai", 403, &Value::Null).contains("Zugriff verweigert"));
    }
}
