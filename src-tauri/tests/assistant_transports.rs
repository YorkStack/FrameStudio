use framestudio::{
    core::Core,
    transport::{self, Bridge},
};
use rmcp::{ServiceExt, model::CallToolRequestParams, transport::TokioChildProcess};
use serde_json::{Value, json};

#[test]
fn schemas_separate_image_generation_from_prompt_assistance() {
    let image = transport::operation_schema("job.submit");
    assert!(
        !image["properties"]["provider"]["enum"]
            .as_array()
            .unwrap()
            .contains(&json!("anthropic"))
    );
    let assistant = transport::operation_schema("assistant.submit");
    assert_eq!(
        assistant["properties"]["provider"]["enum"],
        json!(["anthropic", "test"])
    );
    assert!(
        assistant["required"]
            .as_array()
            .unwrap()
            .contains(&json!("idempotency_key"))
    );
    for schema in [image, assistant] {
        assert_eq!(
            schema["properties"]["expected_price_version"]["type"],
            "string"
        );
        assert!(schema["properties"].get("key").is_none());
    }
    let refresh = transport::operation_schema("pricing.refresh");
    assert_eq!(refresh["additionalProperties"], false);
    assert!(refresh["properties"].get("url").is_none());
}

#[tokio::test]
async fn rest_catalog_and_assistant_use_shared_spend_rules() {
    let dir = tempfile::tempdir().unwrap();
    let core = Core::open(dir.path().into()).unwrap();
    core.call("settings.save", json!({"rest_enabled":true}), false)
        .await
        .unwrap();
    let bridge = Bridge::new(core.clone()).unwrap();
    let token = bridge.create_token().unwrap()["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let server = tokio::spawn(transport::serve_http(bridge, 43131));
    let client = reqwest::Client::new();
    let url = "http://127.0.0.1:43131/rpc";
    for _ in 0..50 {
        if client.get(url).send().await.is_ok() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let catalog = client
        .post(url)
        .bearer_auth(&token)
        .json(&json!({"op":"pricing.catalog","args":{}}))
        .send()
        .await
        .unwrap();
    assert!(catalog.status().is_success());
    assert_eq!(
        catalog.json::<Value>().await.unwrap()["providers"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    // Invalid selectors fail locally and cannot turn refresh into arbitrary URL fetches.
    let bad = client
        .post(url)
        .bearer_auth(&token)
        .json(&json!({"op":"pricing.refresh","args":{"provider":"https://evil.example"}}))
        .send()
        .await
        .unwrap();
    assert_eq!(bad.status(), 400);
    let project = core
        .call(
            "project.save",
            json!({"name":"Synthetic assistant REST"}),
            false,
        )
        .await
        .unwrap();
    let request = json!({"project_id":project["id"],"text":"Eine Werkstatt mit warmem Licht.","idempotency_key":"rest-assistant"});
    let denied = client
        .post(url)
        .bearer_auth(&token)
        .json(&json!({"op":"assistant.submit","args":request}))
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), 400);
    assert!(
        core.call("job.list", json!({}), false)
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty()
    );
    let mut test_request = request.clone();
    test_request["provider"] = json!("test");
    let response = client
        .post(url)
        .bearer_auth(&token)
        .json(&json!({"op":"assistant.submit","args":test_request}))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    let job = response.json::<Value>().await.unwrap()["jobs"][0].clone();
    assert_eq!(job["kind"], "prompt_assist");
    assert!(core.work_once().await.unwrap());
    let result = core
        .call("job.get", json!({"id":job["id"]}), false)
        .await
        .unwrap();
    assert_eq!(result["status"], "completed");
    assert!(!result["result_text"].as_str().unwrap().is_empty());
    assert_eq!(
        core.call("asset.list", json!({"project_id":project["id"]}), false)
            .await
            .unwrap()["total"],
        0
    );
    server.abort();
}

#[tokio::test]
async fn real_mcp_assistant_result_and_backup_remain_text_without_auto_generation() {
    let dir = tempfile::tempdir().unwrap();
    let core = Core::open(dir.path().into()).unwrap();
    core.call("settings.save", json!({"mcp_enabled":true}), false)
        .await
        .unwrap();
    let project = core
        .call(
            "project.save",
            json!({"name":"Synthetic assistant MCP"}),
            false,
        )
        .await
        .unwrap();
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_framestudio"));
    command.args(["--mcp", "--data-dir", dir.path().to_str().unwrap()]);
    let access = Bridge::new(core.clone())
        .unwrap()
        .call(
            "tokens.create",
            json!({"scope":"mcp","name":"Synthetic MCP test"}),
            false,
        )
        .await
        .unwrap();
    command.env("FRAMESTUDIO_MCP_TOKEN", access["token"].as_str().unwrap());
    let client = ().serve(TokioChildProcess::new(command).unwrap()).await.unwrap();
    let tools = client.list_tools(Default::default()).await.unwrap();
    for tool in [
        "pricing_catalog",
        "pricing_refresh",
        "assistant_models",
        "assistant_estimate",
        "assistant_submit",
    ] {
        assert!(tools.tools.iter().any(|item| item.name == tool));
    }
    let request = json!({"project_id":project["id"],"text":"Ein Roboter auf einer Werkbank.","provider":"test","idempotency_key":"mcp-assistant"});
    let submitted = client
        .call_tool(
            CallToolRequestParams::new("assistant_submit")
                .with_arguments(request.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_ne!(submitted.is_error, Some(true));
    let id = submitted.structured_content.unwrap()["jobs"][0]["id"].clone();
    let mut result = Value::Null;
    for _ in 0..100 {
        let reply = client
            .call_tool(
                CallToolRequestParams::new("job_get")
                    .with_arguments(json!({"id":id}).as_object().unwrap().clone()),
            )
            .await
            .unwrap();
        result = reply.structured_content.unwrap();
        if result["status"] == "completed" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(result["status"], "completed");
    assert!(!result["result_text"].as_str().unwrap().is_empty());
    let repeated = client
        .call_tool(
            CallToolRequestParams::new("assistant_submit")
                .with_arguments(request.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_eq!(repeated.structured_content.unwrap()["jobs"][0]["id"], id);
    let archive = core
        .call(
            "export.create",
            json!({"project_id":project["id"],"format":"zip","include_metadata":true}),
            false,
        )
        .await
        .unwrap();
    let data = core
        .call("export.read", json!({"id":archive["id"]}), false)
        .await
        .unwrap();
    let restored = core
        .call(
            "project.restore",
            json!({"data_base64":data["data_base64"]}),
            false,
        )
        .await
        .unwrap();
    let jobs = core
        .call("job.list", json!({"project_id":restored["id"]}), false)
        .await
        .unwrap();
    assert_eq!(jobs[0]["kind"], "prompt_assist");
    assert_eq!(jobs[0]["result_text"], result["result_text"]);
    for field in ["kind", "system", "parameters", "price_snapshot"] {
        assert_eq!(jobs[0]["snapshot"][field], result["snapshot"][field]);
    }
    assert_eq!(jobs[0]["status"], "restored");
    assert_eq!(
        core.call("asset.list", json!({"project_id":restored["id"]}), false)
            .await
            .unwrap()["total"],
        0
    );
    let shared = core
        .call(
            "export.create",
            json!({"project_id":project["id"],"format":"zip","include_metadata":false}),
            false,
        )
        .await
        .unwrap();
    let shared_bytes = core
        .call("export.read", json!({"id":shared["id"]}), false)
        .await
        .unwrap();
    use base64::Engine;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(shared_bytes["data_base64"].as_str().unwrap())
        .unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(decoded)).unwrap();
    let mut manifest = String::new();
    use std::io::Read;
    zip.by_name("manifest.json")
        .unwrap()
        .read_to_string(&mut manifest)
        .unwrap();
    assert!(!manifest.contains("Ein Roboter auf einer Werkbank"));
    assert!(!manifest.contains("result_text"));
    assert_eq!(
        serde_json::from_str::<Value>(&manifest).unwrap()["jobs"],
        json!([])
    );
    client.cancel().await.unwrap();
}
