#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]
use anyhow::{Context, Result};
use framestudio::{
    core::Core,
    transport::{self, Bridge},
};
use std::path::PathBuf;
fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help") {
        println!(
            "FrameStudio [--data-dir PATH] [--serve | --mcp | --rpc | --openapi]\nDefault: desktop. --rpc reads one JSON {{op,args}} from stdin (trusted local administration). REST/MCP require prior activation in Settings. MCP also requires a scoped FRAMESTUDIO_MCP_TOKEN environment variable. No secrets on command line."
        );
        return Ok(());
    }
    if args.iter().any(|a| a == "--openapi") {
        println!("{}", transport::openapi());
        return Ok(());
    }
    let root = match args.iter().position(|a| a == "--data-dir") {
        Some(i) => PathBuf::from(args.get(i + 1).context("--data-dir benötigt einen Pfad")?),
        None => transport::default_root()?,
    };
    let core = Core::open(root)?;
    let bridge = Bridge::new(core.clone())?;
    let runtime = tokio::runtime::Runtime::new()?;
    if args.iter().any(|a| a == "--rpc") {
        return runtime.block_on(async {
            use std::io::Read;
            let mut raw = String::new();
            std::io::stdin()
                .take(90 * 1024 * 1024)
                .read_to_string(&mut raw)?;
            let request: serde_json::Value = serde_json::from_str(&raw)?;
            let value = bridge
                .call(
                    request["op"].as_str().context("op fehlt")?,
                    request["args"].clone(),
                    false,
                )
                .await?;
            println!("{value}");
            Ok(())
        });
    }
    if args
        .iter()
        .any(|a| a == "--serve" || a == "--mcp" || a == "--worker")
    {
        return runtime.block_on(async {
            if args.iter().any(|a| a == "--worker") {
                transport::worker(core).await;
                return Ok(());
            }
            if args.iter().any(|a| a == "--mcp") {
                transport::serve_mcp(bridge).await
            } else {
                tokio::spawn(transport::worker(core));
                transport::serve_http(bridge, 43120).await
            }
        });
    }
    #[cfg(feature = "desktop")]
    {
        framestudio::desktop(bridge)
    }
    #[cfg(not(feature = "desktop"))]
    {
        anyhow::bail!("Desktop-Feature fehlt. --serve oder --mcp verwenden.")
    }
}
