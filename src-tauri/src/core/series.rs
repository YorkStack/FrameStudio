//! Frozen series with explicit, quoted model selection at local approval.
use super::*;
use anyhow::ensure;

pub(super) fn submit(conn: &mut Connection, args: &Value, origin: Value) -> Result<Value> {
    let object = args
        .as_object()
        .context("Serienauftrag muss ein Objekt sein")?;
    ensure!(
        object.keys().all(
            |k| ["project_id", "description", "items", "idempotency_key"].contains(&k.as_str())
        ),
        "Unbekanntes Feld im Serienauftrag; Herkunft wird vom Server ermittelt"
    );
    let key = required(args, "idempotency_key")?;
    ensure!(key.len() <= 200, "Idempotenzschlüssel zu lang");
    let description = required(args, "description")?;
    ensure!(
        description.len() <= 1000,
        "Serienbeschreibung darf höchstens 1000 Bytes enthalten"
    );
    let items = args["items"]
        .as_array()
        .context("Konkrete Bildliste fehlt")?;
    ensure!(
        !items.is_empty() && items.len() <= 32,
        "Eine Serie enthält 1 bis 32 Bilder"
    );
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    // Credential identity scopes client-chosen idempotency keys; metadata changes
    // on the token cannot create a second approval or change historical provenance.
    let identity = json!({"channel":origin["channel"],"access_id":origin["access_id"],"key":key});
    let id = fingerprint(&identity)?;
    let request_fingerprint = fingerprint(args)?;
    if let Some(existing) = find(&tx, "series", &id)? {
        ensure!(
            existing["request_fingerprint"] == request_fingerprint,
            "Idempotenzschlüssel bereits für eine andere Serie verwendet"
        );
        return view(&tx, existing);
    }
    let project = get(&tx, "project", required(args, "project_id")?)?;
    let pending: i64 = tx.query_row("SELECT COUNT(*) FROM records WHERE kind='series' AND json_extract(data,'$.status')='pending'", [], |r|r.get(0))?;
    ensure!(pending < 100, "Höchstens 100 offene Serienfreigaben");
    let mut jobs = Vec::new();
    let mut total = 0i64;
    for item in items {
        let mut input = item.clone();
        let obj = input
            .as_object_mut()
            .context("Jedes Serienbild muss ein Objekt sein")?;
        ensure!(
            !obj.contains_key("project_id") && !obj.contains_key("idempotency_key"),
            "Projekt und Idempotenz werden für die gesamte Serie festgelegt"
        );
        obj.insert("project_id".into(), args["project_id"].clone());
        let count = job_count(&input)?;
        ensure!(
            jobs.len() + count <= 32,
            "Höchstens 32 Bilder einschließlich aller Varianten"
        );
        let variants = array(&input, "variants");
        for i in 0..count {
            let snapshot = build_snapshot(&tx, &variant_input(&input, variants.get(i))?)?;
            let estimate = estimate_snapshot(&snapshot, false)?;
            if input
                .get("expected_price_version")
                .is_some_and(|v| *v != estimate["price_version"])
            {
                bail!("Preise geändert; Serie mit aktueller Schätzung neu einreichen");
            }
            let amount = estimate["total_microusd"]
                .as_i64()
                .context("Unbekannter Preis; keine freigabefähige Serie")?;
            validate_money(&json!(amount))?;
            total = total.checked_add(amount).context("Kostenüberlauf")?;
            jobs.push(json!({"snapshot":snapshot,"estimated_microusd":amount}));
        }
    }
    let record = json!({"id":id,"project_id":args["project_id"],"project_name":project["name"],"description":description,"origin":origin,"created_at":now(),"status":"pending","request_fingerprint":request_fingerprint,"count":jobs.len(),"estimated_microusd":total,"currency":"USD","items":jobs,"job_ids":[],"approved_at":null});
    put(&tx, "series", &record)?;
    tx.commit()?;
    view(conn, record)
}

pub(super) fn view(conn: &Connection, mut series: Value) -> Result<Value> {
    let jobs = array(&series, "job_ids")
        .iter()
        .map(|id| get_job(conn, id.as_str().unwrap_or("")))
        .collect::<Result<Vec<_>>>()?;
    let mut counts = serde_json::Map::new();
    for job in &jobs {
        let status = text(job, "status");
        let count = counts.get(status).and_then(Value::as_u64).unwrap_or(0);
        counts.insert(status.into(), json!(count + 1));
    }
    series["job_status_counts"] = json!(counts);
    series["jobs"] = json!(jobs);
    let settings = settings(conn)?;
    series["warning"] = json!(
        series["estimated_microusd"].as_i64().unwrap_or(0) > 0
            && series["estimated_microusd"].as_i64().unwrap_or(0)
                >= settings["warning_microusd"].as_i64().unwrap_or(0)
    );
    Ok(series)
}

// Quotes never rebuild prompts from mutable styles/characters. Only the explicitly
// selected provider fields and current price basis may change. No jobs or writes.
pub(super) fn quote(conn: &Connection, args: &Value) -> Result<Value> {
    let series = get(conn, "series", required(args, "id")?)?;
    quote_record(conn, &series, args.get("selection"))
}

fn quote_record(conn: &Connection, series: &Value, selection: Option<&Value>) -> Result<Value> {
    ensure!(
        series["status"] == "pending",
        "Nur offene Serien können neu kalkuliert werden"
    );
    let catalog = price_catalog(conn)?;
    let mut items = array(series, "items");
    if let Some(selection) = selection {
        let fields = selection
            .as_object()
            .context("Modellauswahl muss ein Objekt sein")?;
        ensure!(
            fields
                .keys()
                .all(|k| ["provider", "model", "resolution", "quality"].contains(&k.as_str())),
            "Nicht erlaubtes Feld in der Modellauswahl"
        );
        required(selection, "provider")?;
        required(selection, "model")?;
        required(selection, "resolution")?;
        for value in fields.values() {
            ensure!(value.is_string(), "Modellparameter müssen Text sein");
        }
    }
    let mut total = 0i64;
    for item in &mut items {
        let snapshot = &mut item["snapshot"];
        if let Some(selection) = selection {
            let parameters = snapshot["parameters"]
                .as_object_mut()
                .context("Modellparameter fehlen")?;
            parameters.remove("quality");
            parameters.remove("expected_price_version");
            for (key, value) in selection.as_object().unwrap() {
                parameters.insert(key.clone(), value.clone());
            }
            if parameters.get("provider").is_some_and(|v| v == "openai")
                && !parameters.contains_key("quality")
            {
                parameters.insert("quality".into(), json!("medium"));
            }
        }
        provider::validate(&snapshot["parameters"])?;
        let models = provider::models();
        let model = models
            .as_array()
            .unwrap()
            .iter()
            .find(|m| {
                m["id"] == snapshot["parameters"]["model"]
                    && m["provider"] == snapshot["parameters"]["provider"]
            })
            .context("Modell fehlt")?;
        ensure!(
            array(snapshot, "references").len()
                <= model["max_references"].as_u64().unwrap_or(0) as usize,
            "Modell unterstützt nicht alle Referenzen dieser Serie"
        );
        snapshot["price_snapshot"] = pricing::snapshot_for(&catalog, &snapshot["parameters"])?;
        let amount = estimate_snapshot(snapshot, false)?["total_microusd"]
            .as_i64()
            .context("Unbekannter Preis; Freigabe nicht möglich")?;
        validate_money(&json!(amount))?;
        total = total.checked_add(amount).context("Kostenüberlauf")?;
        item["estimated_microusd"] = json!(amount);
    }
    let hash = fingerprint(
        &json!({"id":series["id"],"request":series["request_fingerprint"],"items":items,"total":total}),
    )?;
    let settings = settings(conn)?;
    Ok(
        json!({"id":series["id"],"items":items,"count":items.len(),"estimated_microusd":total,"currency":"USD","quote_id":hash,"job_limit_microusd":settings["job_limit_microusd"],"warning":total >= settings["warning_microusd"].as_i64().unwrap_or(i64::MAX),"selection":selection}),
    )
}

pub(super) fn approve(conn: &mut Connection, args: &Value) -> Result<Value> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut series = get(&tx, "series", required(args, "id")?)?;
    if series["status"] == "approved" {
        return view(&tx, series);
    }
    ensure!(
        series["status"] == "pending",
        "Diese Serie kann nicht mehr freigegeben werden"
    );
    if args.get("selection").is_some() || args.get("expected_quote").is_some() {
        let quote = quote_record(&tx, &series, args.get("selection"))?;
        ensure!(
            args["expected_quote"] == quote["quote_id"],
            "Kostenvorschau geändert; bitte neu prüfen und freigeben"
        );
        series["submitted_items"] = series["items"].clone();
        series["submitted_estimated_microusd"] = series["estimated_microusd"].clone();
        series["items"] = quote["items"].clone();
        series["estimated_microusd"] = quote["estimated_microusd"].clone();
        series["approved_quote_id"] = quote["quote_id"].clone();
        series["approved_selection"] = quote["selection"].clone();
    }
    let project = get(&tx, "project", text(&series, "project_id"))?;
    ensure!(
        project["archived"] != true,
        "Archiviertes Projekt kann nicht starten"
    );
    let session_keys = crate::secrets::session_key_status()?;
    let mut jobs = Vec::new();
    for (index, item) in array(&series, "items").iter().enumerate() {
        let snapshot = &item["snapshot"];
        if snapshot["parameters"]["provider"] != "test" {
            let current = pricing::snapshot_for(&price_catalog(&tx)?, &snapshot["parameters"])?;
            ensure!(
                current == snapshot["price_snapshot"],
                "Preisgrundlage geändert. Serie ablehnen und neu einreichen"
            );
        }
        // References must still exist with the frozen content identity.
        for reference in array(snapshot, "references") {
            let asset = get(&tx, "asset", text(&reference, "id"))?;
            ensure!(
                asset["sha256"] == reference["sha256"]
                    && asset["project_id"] == series["project_id"],
                "Referenzbild geändert; neue Serienfreigabe erforderlich"
            );
        }
        jobs.push(json!({"id":new_id(),"kind":"image_generation","project_id":series["project_id"],"status":"queued","created_at":now(),"idempotency_key":format!("series:{}:{index}",text(&series,"id")),"request_fingerprint":series["request_fingerprint"],"batch_count":series["count"],"series_id":series["id"],"series_index":index,"session_key_required":session_keys["providers"][text(&snapshot["parameters"],"provider")]==true,"origin":series["origin"],"snapshot":snapshot,"estimated_microusd":item["estimated_microusd"],"reserved_microusd":item["estimated_microusd"],"actual_microusd":null,"cost_status":"estimated","error":null,"cancel_requested":false}));
    }
    let one_time_limit = args
        .get("series_limit_microusd")
        .map(|limit| -> Result<i64> {
            validate_money(limit)?;
            ensure!(
                args.get("expected_quote").is_some() && *limit == series["estimated_microusd"],
                "Einmalige Seriengrenze muss der geprüften Gesamtschätzung entsprechen"
            );
            limit.as_i64().context("Ungültige Seriengrenze")
        })
        .transpose()?;
    reserve_jobs_with_limit(
        &tx,
        &jobs,
        series["estimated_microusd"]
            .as_i64()
            .context("Serienkosten fehlen")?,
        text(&series, "project_id"),
        one_time_limit,
    )?;
    series["one_time_limit_microusd"] = json!(one_time_limit);
    series["job_ids"] = json!(jobs.iter().map(|job| job["id"].clone()).collect::<Vec<_>>());
    series["status"] = json!("approved");
    series["approved_at"] = json!(now());
    put(&tx, "series", &series)?;
    tx.commit()?;
    view(conn, series)
}

pub(super) fn cancel(conn: &mut Connection, args: &Value, reject: bool) -> Result<Value> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut series = get(&tx, "series", required(args, "id")?)?;
    if reject {
        ensure!(
            series["status"] != "approved",
            "Freigegebene Serie bitte abbrechen"
        );
    }
    if series["status"] == "pending" || series["status"] == "approved" {
        for id in array(&series, "job_ids") {
            cancel_job(&tx, &json!({"id":id}))?;
        }
        series["status"] = json!(if reject { "rejected" } else { "cancelled" });
        series["closed_at"] = json!(now());
        put(&tx, "series", &series)?;
    }
    tx.commit()?;
    view(conn, series)
}
