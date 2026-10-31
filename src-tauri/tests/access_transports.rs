use framestudio::{
    core::Core,
    transport::{self, Bridge},
};
use rmcp::{ServiceExt, model::CallToolRequestParams, transport::TokioChildProcess};
use serde_json::{Value, json};

async fn access(bridge: &Bridge, scope: &str, name: &str) -> Value {
    bridge.call("tokens.create", json!({"scope":scope,"name":name,"recipient":"Synthetic client","purpose":"Local contract test"}), false).await.unwrap()
}
fn mcp_command(root: &std::path::Path, token: Option<&str>) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_framestudio"));
    command.args(["--mcp", "--data-dir", root.to_str().unwrap()]);
    command.env_remove("FRAMESTUDIO_MCP_TOKEN");
    if let Some(token) = token {
        command.env("FRAMESTUDIO_MCP_TOKEN", token);
    }
    command
}

#[tokio::test]
async fn http_access_is_scoped_independently_revocable_and_never_administrative() {
    let dir = tempfile::tempdir().unwrap();
    let core = Core::open(dir.path().into()).unwrap();
    core.call("settings.save", json!({"rest_enabled":true}), false)
        .await
        .unwrap();
    let bridge = Bridge::new(core).unwrap();
    let first = access(&bridge, "rest", "First REST client").await;
    let second = access(&bridge, "both", "Second client").await;
    let mcp = access(&bridge, "mcp", "MCP only").await;
    let server = tokio::spawn(transport::serve_http(bridge.clone(), 43133));
    let http = reqwest::Client::new();
    let url = "http://127.0.0.1:43133/rpc";
    for _ in 0..50 {
        if http.get(url).send().await.is_ok() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let payload = json!({"op":"project.list","args":{}});
    assert_eq!(
        http.post(url)
            .bearer_auth(mcp["token"].as_str().unwrap())
            .json(&payload)
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        http.post(url)
            .bearer_auth(first["token"].as_str().unwrap())
            .json(&payload)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    for op in [
        "tokens.list",
        "tokens.create",
        "tokens.update",
        "tokens.revoke",
    ] {
        assert_eq!(
            http.post(url)
                .bearer_auth(second["token"].as_str().unwrap())
                .json(&json!({"op":op,"args":{}}))
                .send()
                .await
                .unwrap()
                .status(),
            400
        );
        assert!(!transport::OPERATIONS.contains(&op));
    }
    bridge
        .call("tokens.revoke", json!({"id":first["access"]["id"]}), false)
        .await
        .unwrap();
    assert_eq!(
        http.post(url)
            .bearer_auth(first["token"].as_str().unwrap())
            .json(&payload)
            .send()
            .await
            .unwrap()
            .status(),
        401
    );
    assert_eq!(
        http.post(url)
            .bearer_auth(second["token"].as_str().unwrap())
            .json(&payload)
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let list = bridge.call("tokens.list", json!({}), false).await.unwrap();
    let first_row = list
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == first["access"]["id"])
        .unwrap();
    assert!(first_row["last_used_at"].is_string());
    assert!(first_row["revoked_at"].is_string());
    assert_eq!(first_row["active"], false);
    assert!(!list.to_string().contains(first["token"].as_str().unwrap()));
    server.abort();
}

#[tokio::test]
async fn mcp_requires_scoped_access_and_revocation_applies_to_existing_connections() {
    let dir = tempfile::tempdir().unwrap();
    let core = Core::open(dir.path().into()).unwrap();
    core.call("settings.save", json!({"mcp_enabled":true}), false)
        .await
        .unwrap();
    let bridge = Bridge::new(core).unwrap();
    let rest = access(&bridge, "rest", "REST only").await;
    for token in [None, Some(rest["token"].as_str().unwrap())] {
        let output = mcp_command(dir.path(), token).output().await.unwrap();
        assert!(!output.status.success());
        assert!(!String::from_utf8_lossy(&output.stderr).contains(rest["token"].as_str().unwrap()));
    }
    let first = access(&bridge, "mcp", "MCP first").await;
    let second = access(&bridge, "mcp", "MCP second").await;
    let client = ()
        .serve(
            TokioChildProcess::new(mcp_command(
                dir.path(),
                Some(first["token"].as_str().unwrap()),
            ))
            .unwrap(),
        )
        .await
        .unwrap();
    let other = ()
        .serve(
            TokioChildProcess::new(mcp_command(
                dir.path(),
                Some(second["token"].as_str().unwrap()),
            ))
            .unwrap(),
        )
        .await
        .unwrap();
    let tools = client.list_tools(Default::default()).await.unwrap();
    assert!(!tools.tools.iter().any(|t| t.name.starts_with("tokens_")));
    let reply = client
        .call_tool(CallToolRequestParams::new("project_list"))
        .await
        .unwrap();
    assert_ne!(reply.is_error, Some(true));
    bridge
        .call("tokens.revoke", json!({"id":first["access"]["id"]}), false)
        .await
        .unwrap();
    let denied = client
        .call_tool(CallToolRequestParams::new("project_list"))
        .await
        .unwrap();
    assert_eq!(denied.is_error, Some(true));
    assert!(client.list_tools(Default::default()).await.is_err());
    let allowed = other
        .call_tool(CallToolRequestParams::new("project_list"))
        .await
        .unwrap();
    assert_ne!(allowed.is_error, Some(true));
    assert!(
        !mcp_command(dir.path(), Some(first["token"].as_str().unwrap()))
            .output()
            .await
            .unwrap()
            .status
            .success()
    );
    client.cancel().await.unwrap();
    other.cancel().await.unwrap();
}
