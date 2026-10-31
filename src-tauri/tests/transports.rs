use framestudio::{
    core::Core,
    transport::{self, Bridge},
};
use rmcp::{ServiceExt, model::CallToolRequestParams, transport::TokioChildProcess};
use serde_json::json;

#[tokio::test]
async fn rest_authorization_origin_host_revocation_and_shared_core() {
    let dir = tempfile::tempdir().unwrap();
    let core = Core::open(dir.path().to_path_buf()).unwrap();
    core.call("settings.save", json!({"rest_enabled":true}), false)
        .await
        .unwrap();
    let bridge = Bridge::new(core.clone()).unwrap();
    let token = bridge.create_token().unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(!bridge.authorized("not-a-token"));
    let task = tokio::spawn(transport::serve_http(bridge.clone(), 43129));
    let client = reqwest::Client::new();
    let url = "http://127.0.0.1:43129/rpc";
    for _ in 0..50 {
        if client
            .get("http://127.0.0.1:43129/v1/projects")
            .send()
            .await
            .is_ok()
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let payload = json!({"op":"project.save","args":{"name":"REST project"}});
    assert_eq!(
        client
            .post(url)
            .json(&payload)
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        client
            .post(url)
            .bearer_auth(&token)
            .header("Origin", "https://evil.example")
            .json(&payload)
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        client
            .post(url)
            .bearer_auth(&token)
            .header("Host", "evil.example")
            .json(&payload)
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    let response = client
        .post(url)
        .bearer_auth(&token)
        .json(&payload)
        .send()
        .await
        .unwrap();
    assert!(
        response.status().is_success(),
        "{}",
        response.text().await.unwrap()
    );
    assert_eq!(
        core.call("project.list", json!({}), false)
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let project = core.call("project.list", json!({}), false).await.unwrap()[0].clone();
    let model = framestudio::provider::models()
        .as_array()
        .unwrap()
        .iter()
        .find(|model| model["provider"] == "openai")
        .unwrap()["id"]
        .clone();
    let arguments = json!({"project_id":project["id"],"description":"Synthetic HTTP estimate only","provider":"openai","model":model,"quality":"medium","aspect_ratio":"1:1","resolution":"1K","count":1,"idempotency_key":"rest-openai-denied"});
    let estimate = client
        .post(url)
        .bearer_auth(&token)
        .json(&json!({"op":"cost.estimate","args":arguments}))
        .send()
        .await
        .unwrap();
    assert!(estimate.status().is_success());
    assert!(
        estimate.json::<serde_json::Value>().await.unwrap()["total_microusd"]
            .as_i64()
            .unwrap()
            > 0
    );
    let denied = client
        .post(url)
        .bearer_auth(&token)
        .json(&json!({"op":"job.submit","args":arguments}))
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
    let denied = client
        .post(url)
        .bearer_auth(&token)
        .json(&json!({"op":"settings.save","args":{"automation_approved":true}}))
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), 400);
    let denied = client
        .post(url)
        .bearer_auth(&token)
        .json(&json!({"op":"secret.set","args":{"key":"synthetic-not-saved"}}))
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), 400);
    let style = core.call("style.list", json!({}), false).await.unwrap()[0].clone();
    let prepared = client
        .post(url)
        .bearer_auth(&token)
        .json(&json!({"op":"style.preview.prepare","args":{"id":style["id"]}}))
        .send()
        .await
        .unwrap();
    assert!(prepared.status().is_success());
    let prepared = prepared.json::<serde_json::Value>().await.unwrap();
    assert_eq!(prepared["style"]["id"], style["id"]);
    assert!(!prepared["description"].as_str().unwrap().is_empty());
    let clear = client
        .post(url)
        .bearer_auth(&token)
        .json(&json!({"op":"style.preview.set","args":{"id":style["id"],"asset_id":null}}))
        .send()
        .await
        .unwrap();
    assert!(clear.status().is_success());
    assert!(
        core.call("job.list", json!({}), false)
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty()
    );
    bridge
        .call("tokens.revoke", json!({}), false)
        .await
        .unwrap();
    assert_eq!(
        client
            .post(url)
            .bearer_auth(&token)
            .json(&payload)
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    task.abort();
}

#[tokio::test]
async fn actual_official_mcp_client_connects_to_stdio_binary() {
    let dir = tempfile::tempdir().unwrap();
    let core = Core::open(dir.path().to_path_buf()).unwrap();
    core.call("settings.save", json!({"mcp_enabled":true}), false)
        .await
        .unwrap();
    let mut cmd = tokio::process::Command::new(env!("CARGO_BIN_EXE_framestudio"));
    cmd.args(["--mcp", "--data-dir", dir.path().to_str().unwrap()]);
    let access = Bridge::new(core.clone())
        .unwrap()
        .call(
            "tokens.create",
            json!({"scope":"mcp","name":"Synthetic MCP test"}),
            false,
        )
        .await
        .unwrap();
    cmd.env("FRAMESTUDIO_MCP_TOKEN", access["token"].as_str().unwrap());
    let client = ().serve(TokioChildProcess::new(cmd).unwrap()).await.unwrap();
    let list = client.list_tools(Default::default()).await.unwrap();
    assert!(list.tools.iter().any(|t| t.name == "job_submit"));
    let request = CallToolRequestParams::new("project_save")
        .with_arguments(json!({"name":"MCP project"}).as_object().unwrap().clone());
    let response = client.call_tool(request).await.unwrap();
    assert_ne!(response.is_error, Some(true));
    assert_eq!(
        core.call("project.list", json!({}), false)
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let project = core.call("project.list", json!({}), false).await.unwrap()[0].clone();
    let model = framestudio::provider::models()
        .as_array()
        .unwrap()
        .iter()
        .find(|model| model["provider"] == "openai")
        .unwrap()["id"]
        .clone();
    let arguments = json!({"project_id":project["id"],"description":"Synthetic MCP estimate only","provider":"openai","model":model,"quality":"medium","aspect_ratio":"1:1","resolution":"1K","count":1,"idempotency_key":"mcp-openai-denied"});
    let response = client
        .call_tool(
            CallToolRequestParams::new("cost_estimate")
                .with_arguments(arguments.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_ne!(response.is_error, Some(true));
    let response = client
        .call_tool(
            CallToolRequestParams::new("job_submit")
                .with_arguments(arguments.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_eq!(response.is_error, Some(true));
    assert!(
        core.call("job.list", json!({}), false)
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(list.tools.iter().any(|t| t.name == "style_preview_prepare"));
    assert!(list.tools.iter().any(|t| t.name == "style_preview_set"));
    let style = core.call("style.list", json!({}), false).await.unwrap()[0].clone();
    let prepared = client
        .call_tool(
            CallToolRequestParams::new("style_preview_prepare")
                .with_arguments(json!({"id":style["id"]}).as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_ne!(prepared.is_error, Some(true));
    let cleared = client
        .call_tool(
            CallToolRequestParams::new("style_preview_set").with_arguments(
                json!({"id":style["id"],"asset_id":null})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_ne!(cleared.is_error, Some(true));
    assert!(
        core.call("job.list", json!({}), false)
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty()
    );
    let denied = client
        .call_tool(CallToolRequestParams::new("secret_set"))
        .await;
    assert!(denied.is_err());
    core.call("settings.save", json!({"mcp_enabled":false}), false)
        .await
        .unwrap();
    let response = client
        .call_tool(CallToolRequestParams::new("project_list"))
        .await
        .unwrap();
    assert_eq!(response.is_error, Some(true));
    client.cancel().await.unwrap();
}
