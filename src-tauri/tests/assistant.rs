use framestudio::{assistant, core::Core, pricing};
use serde_json::{Value, json};

async fn setup() -> (tempfile::TempDir, Core, Value) {
    let dir = tempfile::tempdir().unwrap();
    let core = Core::open(dir.path().join("data")).unwrap();
    let project = core
        .call("project.save", json!({"name":"Synthetic assistant"}), false)
        .await
        .unwrap();
    (dir, core, project)
}
fn request(project: &Value, key: &str) -> Value {
    json!({"project_id":project["id"],"text":"York mit weißer Kaffeetasse","instruction":"Klarer Bildausschnitt","provider":"test","model":assistant::TEST_MODEL,"idempotency_key":key})
}
fn response() -> Value {
    json!({"id":"msg_synthetic","type":"message","role":"assistant","model":assistant::MODEL,"content":[{"type":"text","text":"Ein gut sichtbares Motiv."}],"stop_reason":"end_turn","usage":{"input_tokens":100,"output_tokens":20}})
}
#[test]
fn explicit_text_only_request_and_costs() {
    let parameters =
        assistant::parameters(&json!({"text":"York","instruction":"Mehr Kontrast"})).unwrap();
    assert_eq!(parameters["provider"], "anthropic");
    let snap = assistant::snapshot(parameters, pricing::anthropic_snapshot());
    let body = assistant::request_body(&snap);
    assert_eq!(body["model"], assistant::MODEL);
    assert_eq!(body["max_tokens"], 4096);
    assert_eq!(body["thinking"]["type"], "between_tools");
    for key in ["tools", "temperature", "cache_control", "images"] {
        assert!(body.get(key).is_none());
    }
    assert_eq!(body["messages"].as_array().unwrap().len(), 1);
    assert_eq!(body["messages"][0]["content"][0]["type"], "text");
    let estimate = assistant::estimate(&snap).unwrap();
    assert!(estimate["total_microusd"].as_i64().unwrap() > 40960);
    assert_eq!(
        assistant::usage_cost(
            &json!({"input_tokens":100,"output_tokens":20}),
            &snap["price_snapshot"]
        ),
        Some(400)
    );
    for usage in [
        json!({}),
        json!({"input_tokens":100}),
        json!({"input_tokens":-1,"output_tokens":1}),
        json!({"input_tokens":1,"output_tokens":1,"cache_read_input_tokens":1}),
        json!({"input_tokens":1,"output_tokens":1,"server_tool_use":{"web_search_requests":1}}),
    ] {
        assert!(assistant::usage_cost(&usage, &snap["price_snapshot"]).is_none());
    }
    let mut changed = snap["price_snapshot"].clone();
    changed["text_output_microusd_per_million"] = json!(20_000_000);
    assert_eq!(
        assistant::usage_cost(&json!({"input_tokens":100,"output_tokens":20}), &changed),
        Some(600)
    );
}
#[test]
fn rejects_injected_fields_models_bad_types_and_oversize() {
    for input in [
        json!({"text":"","provider":"test"}),
        json!({"text":"ok","provider":"openai"}),
        json!({"text":"ok","model":"fake"}),
        json!({"text":"ok","tools":[]}),
        json!({"text":"ok","reference_ids":[]}),
        json!({"text":"ok","url":"https://evil.invalid"}),
        json!({"text":"ok","instruction":null}),
        json!({"text":"x".repeat(32_001)}),
        json!({"text":"ok","instruction":"x".repeat(8001)}),
    ] {
        assert!(assistant::parameters(&input).is_err(), "{input}");
    }
}
#[test]
fn response_refusal_truncation_unknown_usage_and_invalid_output() {
    let price = pricing::anthropic_snapshot();
    let good = assistant::parse_response(&response(), &price);
    assert_eq!(good.cost, Some(400));
    assert_eq!(good.text.as_deref(), Some("Ein gut sichtbares Motiv."));
    for reason in ["refusal", "max_tokens", "tool_use", "pause_turn"] {
        let mut r = response();
        r["stop_reason"] = json!(reason);
        let out = assistant::parse_response(&r, &price);
        assert!(out.error.is_some());
        assert!(out.text.is_none());
        assert_eq!(out.cost, Some(400));
    }
    for content in [
        json!([]),
        json!([{"type":"image","data":"bad"}]),
        json!([{"type":"text","text":10}]),
        json!([{"type":"thinking","thinking":"private"}]),
    ] {
        let mut r = response();
        r["content"] = content;
        assert!(assistant::parse_response(&r, &price).error.is_some());
    }
    let mut r = response();
    r.as_object_mut().unwrap().remove("usage");
    let out = assistant::parse_response(&r, &price);
    assert!(out.cost.is_none());
    assert!(out.text.is_some());
}
#[tokio::test]
async fn persistent_shared_queue_idempotence_result_and_cancel() {
    let (dir, core, project) = setup().await;
    let args = request(&project, "first");
    let submitted = core
        .call("assistant.submit", args.clone(), true)
        .await
        .unwrap();
    let job = &submitted["jobs"][0];
    assert_eq!(job["kind"], "prompt_assist");
    assert!(
        core.call("assistant.submit", args.clone(), true)
            .await
            .unwrap()["reused"]
            .as_bool()
            .unwrap()
    );
    let mut changed = args.clone();
    changed["text"] = json!("different");
    assert!(core.call("assistant.submit", changed, true).await.is_err());
    assert!(core.work_once().await.unwrap());
    assert!(!core.work_once().await.unwrap());
    let reopened = Core::open(dir.path().join("data")).unwrap();
    let result = reopened
        .call("job.get", json!({"id":job["id"]}), false)
        .await
        .unwrap();
    assert_eq!(result["status"], "completed");
    assert!(
        result["result_text"]
            .as_str()
            .unwrap()
            .contains("Kaffeetasse")
    );
    assert_eq!(result["snapshot"], job["snapshot"]);
    assert!(
        reopened
            .call("asset.list", json!({"project_id":project["id"]}), false)
            .await
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let cost = reopened
        .call("cost.project", json!({"project_id":project["id"]}), false)
        .await
        .unwrap();
    assert_eq!(cost["by_kind"]["prompt_assist"]["calculated_microusd"], 0);
    let next = core
        .call("assistant.submit", request(&project, "cancel"), false)
        .await
        .unwrap();
    core.call("job.cancel", json!({"id":next["jobs"][0]["id"]}), false)
        .await
        .unwrap();
    assert!(!core.work_once().await.unwrap());
}
#[tokio::test]
async fn defaults_cannot_bypass_automation_approval_and_stale_prices_reject() {
    let (_dir, core, project) = setup().await;
    let args =
        json!({"project_id":project["id"],"text":"Synthetic","idempotency_key":"unapproved"});
    let error = core
        .call("assistant.submit", args, true)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("nicht lokal freigegeben"), "{error}");
    let mut request = request(&project, "stale");
    request["expected_price_version"] = json!("outdated");
    assert!(
        core.call("assistant.submit", request.clone(), false)
            .await
            .unwrap_err()
            .to_string()
            .contains("Preise wurden aktualisiert")
    );
    let estimate = core
        .call("assistant.estimate", request.clone(), false)
        .await
        .unwrap();
    request["expected_price_version"] = estimate["price_version"].clone();
    assert!(core.call("assistant.submit", request, false).await.is_ok());
    // Never run a real provider worker; all queued test jobs explicitly select test.
}
#[tokio::test]
async fn paid_reservations_share_project_budget_and_cancel_frees_reservation() {
    let (_dir, core, project) = setup().await;
    let mut args = json!({"project_id":project["id"],"text":"Synthetic","provider":"anthropic","idempotency_key":"paid-reservation"});
    let estimate = core
        .call("assistant.estimate", args.clone(), false)
        .await
        .unwrap();
    let amount = estimate["total_microusd"].as_i64().unwrap();
    core.call(
        "project.save",
        json!({"id":project["id"],"name":"Synthetic","budget_microusd":amount}),
        false,
    )
    .await
    .unwrap();
    let submitted = core
        .call("assistant.submit", args.clone(), false)
        .await
        .unwrap();
    args["idempotency_key"] = json!("second");
    assert!(
        core.call("assistant.submit", args.clone(), false)
            .await
            .unwrap_err()
            .to_string()
            .contains("Projektbudget")
    );
    let cost = core
        .call("cost.project", json!({"project_id":project["id"]}), false)
        .await
        .unwrap();
    assert_eq!(cost["reserved_microusd"], amount);
    assert_eq!(
        cost["by_kind"]["prompt_assist"]["reserved_microusd"],
        amount
    );
    core.call(
        "job.cancel",
        json!({"id":submitted["jobs"][0]["id"]}),
        false,
    )
    .await
    .unwrap();
    let second = core.call("assistant.submit", args, false).await.unwrap();
    core.call("job.cancel", json!({"id":second["jobs"][0]["id"]}), false)
        .await
        .unwrap();
}

#[tokio::test]
async fn interrupted_assistant_is_not_resent_and_keeps_cost_reservation() {
    let (_dir, core, project) = setup().await;
    let args = json!({"project_id":project["id"],"text":"Synthetic interrupted","provider":"anthropic","idempotency_key":"interrupted"});
    let submitted = core.call("assistant.submit", args, false).await.unwrap();
    let job = &submitted["jobs"][0];
    let conn = rusqlite::Connection::open(core.root().join("framestudio.sqlite3")).unwrap();
    conn.execute(
        "UPDATE jobs SET status='running',data=json_set(data,'$.status','running') WHERE id=?1",
        [job["id"].as_str().unwrap()],
    )
    .unwrap();
    drop(conn);
    // Recovery only: no queued real job exists and work_once must not invoke a provider.
    assert!(!core.work_once().await.unwrap());
    let after = core
        .call("job.get", json!({"id":job["id"]}), false)
        .await
        .unwrap();
    assert_eq!(after["status"], "unknown");
    assert_eq!(after["cost_status"], "unknown");
    assert_eq!(after["reserved_microusd"], job["reserved_microusd"]);
    assert!(after["actual_microusd"].is_null());
}
#[tokio::test]
async fn persisted_prices_apply_to_new_jobs_only_and_stale_estimates_reject() {
    let (_dir, core, project) = setup().await;
    let mut args = json!({"project_id":project["id"],"text":"Synthetic price","provider":"anthropic","idempotency_key":"price-old"});
    let estimate = core
        .call("assistant.estimate", args.clone(), false)
        .await
        .unwrap();
    args["expected_price_version"] = estimate["price_version"].clone();
    let submitted = core
        .call("assistant.submit", args.clone(), false)
        .await
        .unwrap();
    core.call(
        "job.cancel",
        json!({"id":submitted["jobs"][0]["id"]}),
        false,
    )
    .await
    .unwrap();
    let mut catalog = core
        .call("pricing.catalog", json!({}), false)
        .await
        .unwrap();
    let price = &mut catalog["providers"][2]["models"][0]["price_snapshot"];
    price["version"] = json!("synthetic-updated-prices");
    price["text_output_microusd_per_million"] = json!(20_000_000);
    let conn = rusqlite::Connection::open(core.root().join("framestudio.sqlite3")).unwrap();
    conn.execute("INSERT OR REPLACE INTO records(kind,id,created_at,data) VALUES('pricing','global','synthetic',?1)",[json!({"id":"global","catalog":catalog}).to_string()]).unwrap();
    drop(conn);
    args["idempotency_key"] = json!("price-new");
    assert!(
        core.call("assistant.submit", args.clone(), false)
            .await
            .unwrap_err()
            .to_string()
            .contains("Preise wurden aktualisiert")
    );
    let new_estimate = core.call("assistant.estimate", args, false).await.unwrap();
    assert!(
        new_estimate["total_microusd"].as_i64().unwrap()
            > estimate["total_microusd"].as_i64().unwrap()
    );
    let old = core
        .call("job.get", json!({"id":submitted["jobs"][0]["id"]}), false)
        .await
        .unwrap();
    assert_eq!(old["snapshot"], submitted["jobs"][0]["snapshot"]);
    let state = core.call("state", json!({}), false).await.unwrap();
    assert!(state["pricing_revision"].is_string());
}
