use framestudio::{core::Core, transport::Bridge};
use serde_json::{Value, json};
use tempfile::tempdir;

async fn setup() -> (tempfile::TempDir, Core, String) {
    let dir = tempdir().unwrap();
    let core = Core::open(dir.path().into()).unwrap();
    let p = core
        .call("project.save", json!({"name":"Series test"}), false)
        .await
        .unwrap();
    (dir, core, p["id"].as_str().unwrap().into())
}
fn request(project: &str, key: &str, paid: bool) -> Value {
    let mut item = json!({"description":"Synthetic picture","provider":"test","model":"deterministic-v1","aspect_ratio":"1:1","resolution":"1K","count":3});
    if paid {
        item["provider"] = json!("openai");
        item["model"] = json!(framestudio::provider::openai::SUNBURST);
    }
    json!({"project_id":project,"description":"Three scenes for a presentation","items":[item],"idempotency_key":key})
}
#[tokio::test]
async fn one_approval_freezes_all_images_and_survives_restart_without_double_start() {
    let (dir, core, project) = setup().await;
    let style = core
        .call(
            "style.save",
            json!({"name":"Frozen style","blocks":{"medium":"Original ink"}}),
            false,
        )
        .await
        .unwrap();
    let mut args = request(&project, "three", false);
    args["items"][0]["style_id"] = style["id"].clone();
    let series = core
        .call("series.submit", args.clone(), true)
        .await
        .unwrap();
    assert_eq!(series["status"], "pending");
    assert_eq!(series["count"], 3);
    assert!(!core.work_once().await.unwrap());
    assert!(
        core.call("job.list", json!({}), false)
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        core.call("series.submit", args.clone(), true)
            .await
            .unwrap()["id"],
        series["id"]
    );
    args["items"][0]["description"] = json!("Changed prompt");
    assert!(core.call("series.submit", args, true).await.is_err());
    core.call(
        "style.save",
        json!({"id":style["id"],"name":"Changed style","blocks":{"medium":"Changed oil"}}),
        false,
    )
    .await
    .unwrap();
    for op in ["series.approve", "series.reject"] {
        assert!(
            core.call(op, json!({"id":series["id"]}), true)
                .await
                .is_err()
        );
    }
    let core = Core::open(dir.path().into()).unwrap();
    let approved = core
        .call("series.approve", json!({"id":series["id"]}), false)
        .await
        .unwrap();
    assert_eq!(approved["jobs"].as_array().unwrap().len(), 3);
    assert!(
        approved["jobs"][0]["snapshot"]["prompt"]
            .as_str()
            .unwrap()
            .contains("Original ink")
    );
    assert_eq!(
        approved["job_ids"],
        core.call("series.approve", json!({"id":series["id"]}), false)
            .await
            .unwrap()["job_ids"]
    );
    for index in 0..3 {
        assert!(core.work_once().await.unwrap());
        let job = core
            .call("job.get", json!({"id":approved["job_ids"][index]}), false)
            .await
            .unwrap();
        assert_eq!(job["status"], "completed");
    }
    assert!(!core.work_once().await.unwrap());
    let final_state = core
        .call("series.get", json!({"id":series["id"]}), false)
        .await
        .unwrap();
    assert_eq!(final_state["job_status_counts"]["completed"], 3);
    assert_eq!(
        core.call("asset.list", json!({"project_id":project}), false)
            .await
            .unwrap()["total"],
        3
    );
}
#[tokio::test]
async fn full_series_budget_is_atomic_and_approval_is_specific_not_global() {
    let (_dir, core, p) = setup().await;
    let first = core
        .call("series.submit", request(&p, "paid-1", true), true)
        .await
        .unwrap();
    let second = core
        .call("series.submit", request(&p, "paid-2", true), true)
        .await
        .unwrap();
    let amount = first["estimated_microusd"].as_i64().unwrap();
    assert!(amount > 0);
    assert_eq!(
        core.call("cost.project", json!({"project_id":p}), false)
            .await
            .unwrap()["reserved_microusd"],
        0
    );
    core.call(
        "project.save",
        json!({"id":p,"name":"Limited","budget_microusd":amount}),
        false,
    )
    .await
    .unwrap();
    let one = core.clone();
    let two = core.clone();
    let id1 = first["id"].clone();
    let id2 = second["id"].clone();
    let a = tokio::spawn(async move { one.call("series.approve", json!({"id":id1}), false).await });
    let b = tokio::spawn(async move { two.call("series.approve", json!({"id":id2}), false).await });
    let results = [a.await.unwrap(), b.await.unwrap()];
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        core.call("job.list", json!({}), false)
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        core.call("cost.project", json!({"project_id":p}), false)
            .await
            .unwrap()["reserved_microusd"],
        amount
    );
    assert_eq!(
        core.call("settings.get", json!({}), false).await.unwrap()["automation_approved"],
        false
    );
    let winner = results.into_iter().find_map(Result::ok).unwrap();
    core.call("series.cancel", json!({"id":winner["id"]}), true)
        .await
        .unwrap();
    assert_eq!(
        core.call("cost.project", json!({"project_id":p}), false)
            .await
            .unwrap()["reserved_microusd"],
        0
    );
    assert!(
        core.call("series.approve", json!({"id":winner["id"]}), false)
            .await
            .is_err()
    );
    // Paid jobs deliberately never executed.
}
#[tokio::test]
async fn limits_rejection_archive_and_client_origin_spoofing_cannot_start_jobs() {
    let (_dir, core, p) = setup().await;
    let mut args = request(&p, "bad", false);
    args["origin"] = json!({"channel":"local"});
    assert!(core.call("series.submit", args, true).await.is_err());
    let mut args = request(&p, "too-many", false);
    args["items"] = json!(vec![args["items"][0].clone(); 11]);
    assert!(core.call("series.submit", args, true).await.is_err());
    let series = core
        .call("series.submit", request(&p, "reject", false), true)
        .await
        .unwrap();
    core.call("series.reject", json!({"id":series["id"]}), false)
        .await
        .unwrap();
    assert!(
        core.call("series.approve", json!({"id":series["id"]}), false)
            .await
            .is_err()
    );
    let series = core
        .call("series.submit", request(&p, "archive", false), true)
        .await
        .unwrap();
    core.call(
        "project.save",
        json!({"id":p,"name":"Archived","archived":true}),
        false,
    )
    .await
    .unwrap();
    assert!(
        core.call("series.approve", json!({"id":series["id"]}), false)
            .await
            .is_err()
    );
    assert!(
        core.call("job.list", json!({}), false)
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty()
    );
}
#[tokio::test]
async fn authenticated_origin_is_historical_scoped_and_contains_no_token() {
    let (_dir, core, p) = setup().await;
    let bridge = Bridge::new(core).unwrap();
    let token = bridge
        .call(
            "tokens.create",
            json!({"name":"Agent studio","recipient":"York","scope":"both"}),
            false,
        )
        .await
        .unwrap();
    let raw = token["token"].as_str().unwrap();
    let args = request(&p, "same-key", false);
    let series = bridge
        .call_authenticated("series.submit", args.clone(), raw, "rest")
        .await
        .unwrap();
    assert_eq!(series["origin"]["name"], "Agent studio");
    assert_eq!(series["origin"]["recipient"], "York");
    assert_eq!(series["origin"]["channel"], "rest");
    assert!(!series.to_string().contains(raw));
    let mcp = bridge
        .call_authenticated("series.submit", args.clone(), raw, "mcp")
        .await
        .unwrap();
    assert_ne!(series["id"], mcp["id"]);
    bridge
        .call(
            "tokens.update",
            json!({"id":token["access"]["id"],"name":"Renamed"}),
            false,
        )
        .await
        .unwrap();
    assert_eq!(
        bridge
            .call_authenticated("series.submit", args.clone(), raw, "rest")
            .await
            .unwrap()["origin"]["name"],
        "Agent studio"
    );
    assert!(
        bridge
            .call_authenticated("series.approve", json!({"id":series["id"]}), raw, "rest")
            .await
            .is_err()
    );
    bridge
        .call("tokens.revoke", json!({"id":token["access"]["id"]}), false)
        .await
        .unwrap();
    assert!(
        bridge
            .call_authenticated("series.submit", args, raw, "rest")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn changed_prices_and_job_limits_require_a_new_review_without_partial_jobs() {
    let (dir, core, p) = setup().await;
    let series = core
        .call("series.submit", request(&p, "price-change", true), true)
        .await
        .unwrap();
    core.call("settings.save", json!({"job_limit_microusd":1}), false)
        .await
        .unwrap();
    assert!(
        core.call("series.approve", json!({"id":series["id"]}), false)
            .await
            .unwrap_err()
            .to_string()
            .contains("Auftragsbudget")
    );
    core.call(
        "settings.save",
        json!({"job_limit_microusd":1_000_000}),
        false,
    )
    .await
    .unwrap();
    // Local synthetic catalog change, never a remote price fetch.
    let mut catalog = core
        .call("pricing.catalog", json!({}), false)
        .await
        .unwrap();
    // Discover the bundled shape explicitly: the OpenAI snapshot lives under providers.
    fn change_openai(value: &mut Value) {
        if let Some(obj) = value.as_object_mut() {
            if obj.get("provider").is_some_and(|v| v == "openai") && obj.contains_key("version") {
                obj.insert("version".into(), json!("synthetic-price-change"));
            }
            for v in obj.values_mut() {
                change_openai(v);
            }
        } else if let Some(arr) = value.as_array_mut() {
            for v in arr {
                change_openai(v);
            }
        }
    }
    change_openai(&mut catalog);
    let db = rusqlite::Connection::open(dir.path().join("framestudio.sqlite3")).unwrap();
    db.execute("INSERT OR REPLACE INTO records(kind,id,project_id,created_at,data) VALUES('pricing','global','','2026-10-31',?1)",[json!({"id":"global","catalog":catalog}).to_string()]).unwrap();
    assert!(
        core.call("series.approve", json!({"id":series["id"]}), false)
            .await
            .unwrap_err()
            .to_string()
            .contains("Preisgrundlage geändert")
    );
    assert!(
        core.call("job.list", json!({}), false)
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        core.call("series.get", json!({"id":series["id"]}), false)
            .await
            .unwrap()["status"],
        "pending"
    );
}

#[tokio::test]
async fn session_bound_series_cannot_be_claimed_by_a_worker_without_that_session_key() {
    let (_dir, core, p) = setup().await;
    let series = core
        .call("series.submit", request(&p, "session-series", true), true)
        .await
        .unwrap();
    framestudio::secrets::set_session_key_for("openai", "synthetic-series-session-key-never-sent")
        .unwrap();
    let approved = core
        .call("series.approve", json!({"id":series["id"]}), false)
        .await
        .unwrap();
    framestudio::secrets::remove_session_key_for("openai").unwrap();
    assert!(
        approved["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|j| j["session_key_required"] == true)
    );
    assert!(!core.work_once().await.unwrap());
    assert_eq!(
        core.call("series.get", json!({"id":series["id"]}), false)
            .await
            .unwrap()["job_status_counts"]["queued"],
        3
    );
    core.call("series.cancel", json!({"id":series["id"]}), false)
        .await
        .unwrap();
}

#[tokio::test]
async fn model_quotes_preserve_frozen_content_and_approval_uses_exact_reviewed_price() {
    let (_dir, core, p) = setup().await;
    let style = core
        .call(
            "style.save",
            json!({"name":"Ink","blocks":{"medium":"Frozen ink"}}),
            false,
        )
        .await
        .unwrap();
    let mut args = request(&p, "compare", false);
    args["items"][0]["style_id"] = style["id"].clone();
    let series = core
        .call("series.submit", args.clone(), true)
        .await
        .unwrap();
    core.call(
        "style.save",
        json!({"id":style["id"],"name":"Changed","blocks":{"medium":"New paint"}}),
        false,
    )
    .await
    .unwrap();
    let selection = json!({"provider":"openai","model":framestudio::provider::openai::SUNBURST,"resolution":"1K","quality":"low"});
    let quote = core
        .call(
            "series.quote",
            json!({"id":series["id"],"selection":selection}),
            true,
        )
        .await
        .unwrap();
    assert_eq!(quote["count"], 3);
    assert!(quote["estimated_microusd"].as_i64().unwrap() > 0);
    for (original, updated) in series["items"]
        .as_array()
        .unwrap()
        .iter()
        .zip(quote["items"].as_array().unwrap())
    {
        for field in [
            "prompt",
            "style",
            "character",
            "references",
            "blocks",
            "project",
        ] {
            assert_eq!(original["snapshot"][field], updated["snapshot"][field]);
        }
        assert_eq!(updated["snapshot"]["parameters"]["quality"], "low");
    }
    assert!(
        core.call("job.list", json!({}), false)
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        core.call("series.get", json!({"id":series["id"]}), false)
            .await
            .unwrap()["items"],
        series["items"]
    );
    // A choice must be tied to a reviewed quote, never silently accepted.
    assert!(
        core.call(
            "series.approve",
            json!({"id":series["id"],"selection":selection}),
            false
        )
        .await
        .is_err()
    );
    let mut changed = selection.clone();
    changed["quality"] = json!("high");
    assert!(
        core.call(
            "series.approve",
            json!({"id":series["id"],"selection":changed,"expected_quote":quote["quote_id"]}),
            false
        )
        .await
        .is_err()
    );
    core.call("settings.save", json!({"job_limit_microusd":1}), false)
        .await
        .unwrap();
    let approval = json!({"id":series["id"],"selection":selection,"expected_quote":quote["quote_id"],"series_limit_microusd":quote["estimated_microusd"]});
    assert!(
        core.call("series.approve", approval.clone(), true)
            .await
            .is_err()
    );
    let approved = core
        .call("series.approve", approval.clone(), false)
        .await
        .unwrap();
    assert_eq!(approved["submitted_items"], series["items"]);
    assert_eq!(approved["items"], quote["items"]);
    assert_eq!(approved["origin"], series["origin"]);
    assert_eq!(
        approved["jobs"][0]["snapshot"]["parameters"]["provider"],
        "openai"
    );
    assert_eq!(
        core.call("settings.get", json!({}), false).await.unwrap()["job_limit_microusd"],
        1
    );
    assert_eq!(
        core.call("series.approve", approval, false).await.unwrap()["job_ids"],
        approved["job_ids"]
    );
    assert_eq!(
        core.call("series.submit", args, true).await.unwrap()["id"],
        approved["id"]
    );
    // Paid jobs never executed.
    core.call("series.cancel", json!({"id":series["id"]}), false)
        .await
        .unwrap();
}

#[tokio::test]
async fn model_quotes_reject_incompatible_formats_injection_and_cannot_bypass_project_budget() {
    let (_dir, core, p) = setup().await;
    let mut args = request(&p, "incompatible", false);
    args["items"][0]["aspect_ratio"] = json!("16:9");
    let series = core.call("series.submit", args, true).await.unwrap();
    let selection = json!({"provider":"openai","model":framestudio::provider::openai::SUNBURST,"resolution":"1K","quality":"medium"});
    assert!(
        core.call(
            "series.quote",
            json!({"id":series["id"],"selection":selection}),
            true
        )
        .await
        .is_err()
    );
    let mut injected =
        json!({"provider":"test","model":"deterministic-v1","resolution":"1K","reference_ids":[]});
    assert!(
        core.call(
            "series.quote",
            json!({"id":series["id"],"selection":injected}),
            false
        )
        .await
        .is_err()
    );
    injected.as_object_mut().unwrap().remove("reference_ids");
    injected["quality"] = json!("medium");
    assert!(
        core.call(
            "series.quote",
            json!({"id":series["id"],"selection":injected}),
            false
        )
        .await
        .is_err()
    );
    let paid = core
        .call("series.submit", request(&p, "budget", true), true)
        .await
        .unwrap();
    let quote = core
        .call("series.quote", json!({"id":paid["id"]}), true)
        .await
        .unwrap();
    core.call(
        "project.save",
        json!({"id":p,"name":"Tiny budget","budget_microusd":1}),
        false,
    )
    .await
    .unwrap();
    let error = core.call("series.approve",json!({"id":paid["id"],"expected_quote":quote["quote_id"],"series_limit_microusd":quote["estimated_microusd"]}),false).await.unwrap_err();
    assert!(error.to_string().contains("Projektbudget"));
    assert!(
        core.call("job.list", json!({}), false)
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn series_asset_filter_is_ordered_paged_and_project_scoped() {
    let (_dir, core, project) = setup().await;
    let series = core
        .call(
            "series.submit",
            request(&project, "export-series", false),
            false,
        )
        .await
        .unwrap();
    let approved = core
        .call("series.approve", json!({"id":series["id"]}), false)
        .await
        .unwrap();
    for _ in 0..3 {
        assert!(core.work_once().await.unwrap());
    }
    core.call("job.submit",json!({"project_id":project,"provider":"test","description":"Unrelated image","idempotency_key":"other-image"}),false).await.unwrap();
    assert!(core.work_once().await.unwrap());
    let all = core
        .call("asset.list", json!({"project_id":project}), false)
        .await
        .unwrap();
    assert_eq!(all["total"], 4);
    let selected = core
        .call(
            "asset.list",
            json!({"project_id":project,"series_id":series["id"]}),
            false,
        )
        .await
        .unwrap();
    assert_eq!(selected["total"], 3);
    for i in 0..3 {
        assert_eq!(selected["items"][i]["job_id"], approved["job_ids"][i]);
    }
    let page = core
        .call(
            "asset.list",
            json!({"project_id":project,"series_id":series["id"],"offset":1,"limit":1}),
            false,
        )
        .await
        .unwrap();
    assert_eq!(page["items"][0]["id"], selected["items"][1]["id"]);
    let other = core
        .call("project.save", json!({"name":"Other"}), false)
        .await
        .unwrap();
    assert!(
        core.call(
            "asset.list",
            json!({"project_id":other["id"],"series_id":series["id"]}),
            false
        )
        .await
        .is_err()
    );
    assert!(
        core.call(
            "asset.list",
            json!({"project_id":project,"series_id":123}),
            false
        )
        .await
        .is_err()
    );
}
