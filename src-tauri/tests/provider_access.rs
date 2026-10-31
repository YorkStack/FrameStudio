use framestudio::{
    core::Core,
    transport::{Bridge, operation_schema},
};
use serde_json::json;

#[tokio::test]
async fn secret_routes_validate_provider_and_type_without_touching_keys() {
    let directory = tempfile::tempdir().unwrap();
    let bridge = Bridge::new(Core::open(directory.path().into()).unwrap()).unwrap();
    for provider in [
        json!("unknown"),
        json!("../openai"),
        json!(false),
        json!(null),
    ] {
        for operation in ["secret.set", "secret.delete"] {
            assert!(
                bridge
                    .call(
                        operation,
                        json!({"provider":provider,"key":"synthetic-never-stored"}),
                        false
                    )
                    .await
                    .is_err()
            );
        }
    }
    for provider in ["gemini", "openai"] {
        let error = bridge
            .call(
                "secret.set",
                json!({"provider":provider,"key":"bad"}),
                false,
            )
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), "Ungültiges Schlüsselformat");
        for operation in ["secret.set", "secret.delete"] {
            let error = bridge
                .call(
                    operation,
                    json!({"provider":provider,"key":"synthetic-never-stored"}),
                    true,
                )
                .await
                .unwrap_err();
            assert!(error.to_string().contains("nur in der Desktop-App"));
        }
    }
    // An omitted provider remains a backward-compatible Gemini command.
    let error = bridge
        .call("secret.set", json!({"key":"bad"}), false)
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "Ungültiges Schlüsselformat");
}

#[tokio::test]
async fn public_status_exposes_only_presence_store_and_availability_for_each_provider() {
    let directory = tempfile::tempdir().unwrap();
    let core = Core::open(directory.path().into()).unwrap();
    let state = core.call("state", json!({}), true).await.unwrap();
    for provider in ["gemini", "openai"] {
        let status = &state["key_status"]["providers"][provider];
        assert!(status["configured"].is_boolean());
        assert!(status["store"].is_string());
        assert!(matches!(
            status["status"].as_str(),
            Some("configured" | "missing" | "unavailable")
        ));
        assert_eq!(status["configured"], status["status"] == "configured");
        assert_eq!(status.as_object().unwrap().len(), 3);
    }
    assert_eq!(
        state["key_status"]["configured"],
        state["key_status"]["providers"]["gemini"]["configured"]
    );
    assert!(
        state["models"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["provider"] == "openai")
    );
}

#[test]
fn transport_schemas_advertise_openai_and_quality_without_key_operations() {
    for operation in ["prompt.preview", "cost.estimate", "job.submit"] {
        let schema = operation_schema(operation);
        assert!(
            schema["properties"]["provider"]["enum"]
                .as_array()
                .unwrap()
                .contains(&json!("openai"))
        );
        assert!(
            schema["properties"]["quality"]["enum"]
                .as_array()
                .unwrap()
                .contains(&json!("medium"))
        );
        assert!(schema["properties"].get("key").is_none());
    }
    assert!(!framestudio::transport::OPERATIONS.contains(&"secret.set"));
    assert!(!framestudio::transport::OPERATIONS.contains(&"secret.delete"));
}

#[tokio::test]
async fn backup_keeps_openai_quality_and_price_snapshot_without_resuming_jobs() {
    let directory = tempfile::tempdir().unwrap();
    let core = Core::open(directory.path().into()).unwrap();
    let project = core
        .call(
            "project.save",
            json!({"name":"Synthetic OpenAI backup"}),
            false,
        )
        .await
        .unwrap();
    let model = framestudio::provider::models()
        .as_array()
        .unwrap()
        .iter()
        .find(|model| model["provider"] == "openai")
        .unwrap()["id"]
        .clone();
    let submitted = core.call("job.submit", json!({"project_id":project["id"],"description":"Synthetic metadata only; no worker","provider":"openai","model":model,"quality":"high","aspect_ratio":"3:2","resolution":"1K","count":1,"idempotency_key":"archive-openai"}), false).await.unwrap();
    let snapshot = submitted["jobs"][0]["snapshot"].clone();
    let exported = core
        .call(
            "export.create",
            json!({"project_id":project["id"],"format":"zip","include_metadata":true}),
            false,
        )
        .await
        .unwrap();
    let archive = core
        .call("export.read", json!({"id":exported["id"]}), false)
        .await
        .unwrap();
    let restored = core
        .call(
            "project.restore",
            json!({"data_base64":archive["data_base64"]}),
            false,
        )
        .await
        .unwrap();
    let jobs = core
        .call("job.list", json!({"project_id":restored["id"]}), false)
        .await
        .unwrap();
    assert_eq!(jobs.as_array().unwrap().len(), 1);
    assert_eq!(jobs[0]["status"], "restored");
    assert_eq!(jobs[0]["snapshot"]["parameters"]["quality"], "high");
    assert_eq!(jobs[0]["snapshot"]["parameters"]["model"], model);
    assert_eq!(
        jobs[0]["snapshot"]["price_snapshot"],
        snapshot["price_snapshot"]
    );
    assert_eq!(jobs[0]["snapshot"]["prompt"], snapshot["prompt"]);
    // This test never starts a worker: exporting queued metadata cannot send a provider request.
}

#[test]
fn style_preview_transport_contract_requires_explicit_asset_or_null() {
    let prepare = operation_schema("style.preview.prepare");
    assert_eq!(prepare["required"], json!(["id"]));
    let set = operation_schema("style.preview.set");
    assert_eq!(set["required"], json!(["id", "asset_id"]));
    assert_eq!(
        set["properties"]["asset_id"]["type"],
        json!(["string", "null"])
    );
    assert_eq!(
        operation_schema("style.save")["properties"]["preview_prompt"]["type"],
        "string"
    );
    assert_eq!(
        operation_schema("asset.list")["properties"]["style_id"]["type"],
        "string"
    );
}
