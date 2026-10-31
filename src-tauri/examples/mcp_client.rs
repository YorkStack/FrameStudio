//! cargo run --no-default-features --example mcp_client -- /path/to/framestudio [data-dir]
use rmcp::{ServiceExt, model::CallToolRequestParams, transport::TokioChildProcess};
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let mut cmd = tokio::process::Command::new(
        args.get(1)
            .ok_or_else(|| anyhow::anyhow!("Path to FrameStudio binary required"))?,
    );
    // Inherit from the local host environment; never put the token in command-line arguments.
    anyhow::ensure!(
        std::env::var_os("FRAMESTUDIO_MCP_TOKEN").is_some(),
        "FRAMESTUDIO_MCP_TOKEN is required (create an MCP access in FrameStudio Settings)"
    );
    cmd.arg("--mcp");
    if let Some(root) = args.get(2) {
        cmd.args(["--data-dir", root]);
    }
    let client = ().serve(TokioChildProcess::new(cmd)?).await?;
    println!(
        "Tools: {}",
        client.list_tools(Default::default()).await?.tools.len()
    );
    let result = client
        .call_tool(CallToolRequestParams::new("project_list"))
        .await?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    client.cancel().await?;
    Ok(())
}
