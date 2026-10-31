use framestudio::{
    core::Core,
    transport::{self, Bridge},
};
use rmcp::{ServiceExt, model::CallToolRequestParams, transport::TokioChildProcess};
use serde_json::{Value, json};
fn args(p: &Value) -> Value {
    json!({"project_id":p["id"],"description":"Three scenes via authenticated client","idempotency_key":"transport-series","items":[{"description":"Start","count":1},{"description":"Middle","count":1},{"description":"End","count":1}]})
}
async fn setup() -> (tempfile::TempDir, Bridge, Value, Value) {
    let dir = tempfile::tempdir().unwrap();
    let core = Core::open(dir.path().into()).unwrap();
    core.call(
        "settings.save",
        json!({"rest_enabled":true,"mcp_enabled":true}),
        false,
    )
    .await
    .unwrap();
    let project = core
        .call("project.save", json!({"name":"Transport series"}), false)
        .await
        .unwrap();
    let bridge = Bridge::new(core).unwrap();
    let access = bridge
        .call(
            "tokens.create",
            json!({"scope":"both","name":"Series client","recipient":"Synthetic agent"}),
            false,
        )
        .await
        .unwrap();
    (dir, bridge, project, access)
}
#[tokio::test]
async fn rest_series_summary_and_single_local_approval() {
    let (_dir, bridge, project, access) = setup().await;
    let task = tokio::spawn(transport::serve_http(bridge.clone(), 43139));
    let client = reqwest::Client::new();
    let url = "http://127.0.0.1:43139/rpc";
    for _ in 0..50 {
        if client.get(url).send().await.is_ok() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let token = access["token"].as_str().unwrap();
    let response = client
        .post(url)
        .bearer_auth(token)
        .json(&json!({"op":"series.submit","args":args(&project)}))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    let series: Value = response.json().await.unwrap();
    assert_eq!(series["origin"]["channel"], "rest");
    assert_eq!(series["origin"]["recipient"], "Synthetic agent");
    assert_eq!(series["status"], "pending");
    assert_eq!(series["count"], 3);
    assert_eq!(series["estimated_microusd"], 0);
    assert!(!series.to_string().contains(token));
    let quote: Value = client.post(url).bearer_auth(token).json(&json!({"op":"series.quote","args":{"id":series["id"],"selection":{"provider":"openai","model":framestudio::provider::openai::SUNBURST,"resolution":"1K","quality":"medium"}}})).send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
    assert!(quote["estimated_microusd"].as_i64().unwrap() > 0);
    assert_eq!(quote["count"], 3);

    assert_eq!(
        client
            .post(url)
            .bearer_auth(token)
            .json(&json!({"op":"series.approve","args":{"id":series["id"]}}))
            .send()
            .await
            .unwrap()
            .status(),
        400
    );
    assert!(!bridge.core.work_once().await.unwrap());
    bridge
        .call("series.approve", json!({"id":series["id"]}), false)
        .await
        .unwrap();
    for _ in 0..3 {
        assert!(bridge.core.work_once().await.unwrap());
    }
    let done: Value = client
        .post(url)
        .bearer_auth(token)
        .json(&json!({"op":"series.get","args":{"id":series["id"]}}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(done["job_status_counts"]["completed"], 3);
    let asset: Value = client.post(url).bearer_auth(token)
        .json(&json!({"op":"asset.list","args":{"project_id":project["id"],"series_id":series["id"]}}))
        .send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
    assert_eq!(asset["total"], 3);
    let schema = transport::openapi();
    assert!(schema["paths"]["/v1/operations/series.submit"].is_object());
    assert!(schema["paths"]["/v1/operations/series.approve"].is_null());
    task.abort();
}
#[tokio::test]
async fn actual_mcp_connection_waits_for_one_approval_then_returns_all_results() {
    let (dir, bridge, project, access) = setup().await;
    let mut cmd = tokio::process::Command::new(env!("CARGO_BIN_EXE_framestudio"));
    cmd.args(["--mcp", "--data-dir", dir.path().to_str().unwrap()]);
    cmd.env("FRAMESTUDIO_MCP_TOKEN", access["token"].as_str().unwrap());
    let client = ().serve(TokioChildProcess::new(cmd).unwrap()).await.unwrap();
    let tools = client.list_tools(Default::default()).await.unwrap();
    assert!(tools.tools.iter().any(|t| t.name == "series_submit"));
    assert!(!tools.tools.iter().any(|t| t.name == "series_approve"));
    let reply = client
        .call_tool(
            CallToolRequestParams::new("series_submit")
                .with_arguments(args(&project).as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_ne!(reply.is_error, Some(true));
    let series = reply.structured_content.unwrap();
    assert_eq!(series["origin"]["channel"], "mcp");
    assert!(tools.tools.iter().any(|t| t.name == "series_quote"));
    let quote = client
        .call_tool(
            CallToolRequestParams::new("series_quote")
                .with_arguments(json!({"id":series["id"]}).as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    assert_ne!(quote.is_error, Some(true));
    assert_eq!(quote.structured_content.unwrap()["estimated_microusd"], 0);

    assert_eq!(series["origin"]["name"], "Series client");
    assert_eq!(series["status"], "pending");
    assert_eq!(series["count"], 3);
    assert!(
        bridge
            .call("job.list", json!({}), false)
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty()
    );
    bridge
        .call("series.approve", json!({"id":series["id"]}), false)
        .await
        .unwrap();
    let mut complete = false;
    for _ in 0..100 {
        let result = client
            .call_tool(
                CallToolRequestParams::new("series_get")
                    .with_arguments(json!({"id":series["id"]}).as_object().unwrap().clone()),
            )
            .await
            .unwrap()
            .structured_content
            .unwrap();
        if result["job_status_counts"]["completed"] == 3 {
            complete = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(complete);
    let assets = client
        .call_tool(
            CallToolRequestParams::new("asset_list").with_arguments(
                json!({"project_id":project["id"],"series_id":series["id"]})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap()
        .structured_content
        .unwrap();
    assert_eq!(assets["total"], 3);
    let image = client
        .call_tool(
            CallToolRequestParams::new("asset_read").with_arguments(
                json!({"id":assets["items"][0]["id"],"thumbnail":true})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        )
        .await
        .unwrap();
    assert_ne!(image.is_error, Some(true));
    assert!(
        image.structured_content.unwrap()["data_base64"]
            .as_str()
            .unwrap()
            .len()
            > 100
    );
    client.cancel().await.unwrap();
}
