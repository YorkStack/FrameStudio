//! Local UI test fixture: persistent actual Rust core, no worker or provider calls.
//! Not bundled with the installed app. Only session administration and state allowed.
use framestudio::{core::Core, transport::Bridge};
use serde_json::{Value, json};
use std::io::{BufRead, Write};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let root = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("Test directory required"))?;
    let bridge = Bridge::new(Core::open(root.into())?)?;
    for line in std::io::stdin().lock().lines() {
        let request: Value = serde_json::from_str(&line?)?;
        let op = request["op"].as_str().unwrap_or("");
        let reply = if [
            "state",
            "secret.session.set",
            "secret.session.remove",
            "secret.session.status",
            "secret.check",
        ]
        .contains(&op)
        {
            if op == "secret.check"
                && framestudio::secrets::session_key_status()?["providers"]
                    [request["args"]["provider"].as_str().unwrap_or("")]
                    != true
            {
                anyhow::bail!("This fixture only checks synthetic session keys");
            }
            match bridge.call(op, request["args"].clone(), false).await {
                Ok(value) => json!({"value":value}),
                Err(error) => json!({"error":error.to_string()}),
            }
        } else {
            json!({"error":"Operation disabled in session UI fixture"})
        };
        println!("{reply}");
        std::io::stdout().flush()?;
    }
    Ok(())
}
