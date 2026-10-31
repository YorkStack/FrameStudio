//! Submit a concrete list via the official SDK; never approve or auto-retry.
//! mcp_series_client /path/to/framestudio request.json [data-dir]
use anyhow::{Context, Result, ensure};
use rmcp::{ServiceExt, model::CallToolRequestParams, transport::TokioChildProcess};
use serde_json::{Value, json};
#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let binary = args.get(1).context("FrameStudio-Binary fehlt")?;
    let file = args.get(2).context("JSON-Serienauftrag fehlt")?;
    ensure!(
        std::env::var_os("FRAMESTUDIO_MCP_TOKEN").is_some(),
        "FRAMESTUDIO_MCP_TOKEN fehlt; keine Providerkeys verwenden"
    );
    let payload: Value = serde_json::from_slice(&std::fs::read(file)?)?;
    let mut command = tokio::process::Command::new(binary);
    command.arg("--mcp");
    if let Some(root) = args.get(3) {
        command.args(["--data-dir", root]);
    }
    let client = ().serve(TokioChildProcess::new(command)?).await?;
    let reply = client
        .call_tool(
            CallToolRequestParams::new("series_submit")
                .with_arguments(payload.as_object().context("JSON-Objekt erwartet")?.clone()),
        )
        .await?;
    ensure!(
        reply.is_error != Some(true),
        "Serienauftrag abgelehnt: {:?}",
        reply.content
    );
    let series = reply.structured_content.context("Serienantwort fehlt")?;
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"id":series["id"],"description":series["description"],"origin":series["origin"],"count":series["count"],"estimated_microusd":series["estimated_microusd"],"currency":series["currency"],"status":series["status"],"job_ids":series["job_ids"]})
        )?
    );
    println!(
        "Bei pending: in FrameStudio unter Serienfreigaben einmalig prüfen und freigeben. Status anschließend über series_get mit dieser ID abfragen."
    );
    client.cancel().await?;
    Ok(())
}
