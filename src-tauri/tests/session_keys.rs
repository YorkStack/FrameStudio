use framestudio::{core::Core, secrets, transport::Bridge};
use serde_json::json;

#[tokio::test]
async fn session_keys_are_isolated_redacted_local_only_and_do_not_survive_a_process() {
    let dir = tempfile::tempdir().unwrap();
    let bridge = Bridge::new(Core::open(dir.path().into()).unwrap()).unwrap();
    let first = "synthetic-session-unique-first-8729";
    let replacement = "synthetic-session-unique-replaced-8729";
    for provider in ["gemini", "openai", "anthropic"] {
        let status = bridge
            .call(
                "secret.session.set",
                json!({"provider":provider,"key":first}),
                false,
            )
            .await
            .unwrap();
        assert_eq!(status["providers"][provider], true);
        assert!(!status.to_string().contains(first));
        let check = bridge
            .call("secret.check", json!({"provider":provider}), false)
            .await
            .unwrap();
        assert_eq!(
            check,
            json!({"provider":provider,"readable":true,"provider_contacted":false})
        );
        assert!(!check.to_string().contains(first));
        // Same resolution entry point as both image providers and Claude. This
        // succeeds with an arbitrary synthetic key without reading the OS store.
        assert!(secrets::provider_key_for(provider).unwrap().as_str() == first);
    }
    bridge
        .call(
            "secret.session.set",
            json!({"provider":"gemini","key":replacement}),
            false,
        )
        .await
        .unwrap();
    assert!(secrets::provider_key_for("gemini").unwrap().as_str() == replacement);
    assert!(secrets::provider_key_for("openai").unwrap().as_str() == first);
    let state = bridge.call("state", json!({}), false).await.unwrap();
    for provider in ["gemini", "openai", "anthropic"] {
        assert_eq!(
            state["key_status"]["providers"][provider]["status"],
            "session"
        );
        assert_eq!(
            state["key_status"]["providers"][provider]["configured"],
            true
        );
    }
    assert!(!state.to_string().contains(first));
    assert!(!state.to_string().contains(replacement));
    // A newly started process using the identical on-disk data has no session keys.
    use std::io::Write;
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_framestudio"))
        .args(["--data-dir", dir.path().to_str().unwrap(), "--rpc"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"op":"secret.session.status","args":{}}"#)
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success());
    let fresh: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    for provider in ["gemini", "openai", "anthropic"] {
        assert_eq!(fresh["providers"][provider], false);
    }
    for op in [
        "secret.session.set",
        "secret.session.remove",
        "secret.session.status",
        "secret.check",
    ] {
        assert!(
            bridge
                .call(op, json!({"provider":"gemini","key":first}), true)
                .await
                .is_err()
        );
        assert!(!framestudio::transport::OPERATIONS.contains(&op));
    }
    for provider in ["unknown", "../gemini", "test"] {
        assert!(
            bridge
                .call(
                    "secret.session.set",
                    json!({"provider":provider,"key":first}),
                    false
                )
                .await
                .is_err()
        );
    }
    for key in ["short", "synthetic invalid whitespace", ""] {
        assert!(
            bridge
                .call(
                    "secret.session.set",
                    json!({"provider":"gemini","key":key}),
                    false
                )
                .await
                .is_err()
        );
    }
    assert!(secrets::provider_key_for("gemini").unwrap().as_str() == replacement);
    let project = bridge
        .call(
            "project.save",
            json!({"name":"Session-bound single image"}),
            false,
        )
        .await
        .unwrap();
    let mut pending_ids = Vec::new();
    for provider in ["gemini", "openai"] {
        let model = framestudio::provider::models()
            .as_array()
            .unwrap()
            .iter()
            .find(|model| model["provider"] == provider)
            .unwrap()["id"]
            .clone();
        let submitted = bridge.call("job.submit", json!({"project_id":project["id"],"provider":provider,"model":model,"description":"Synthetic session test, never sent","idempotency_key":format!("session-{provider}")}), false).await.unwrap();
        assert_eq!(submitted["jobs"][0]["session_key_required"], true);
        pending_ids.push(submitted["jobs"][0]["id"].clone());
    }
    for provider in ["gemini", "openai", "anthropic"] {
        let removed = bridge
            .call("secret.session.remove", json!({"provider":provider}), false)
            .await
            .unwrap();
        assert_eq!(removed["providers"][provider], false);
        if provider == "gemini" {
            assert_eq!(removed["providers"]["openai"], true);
        }
    }
    // An external/restarted worker without session keys must neither touch the OS
    // store nor send these requests. Keys are not persisted with the queue.
    assert!(!bridge.core.work_once().await.unwrap());
    for id in pending_ids {
        let job = bridge
            .call("job.get", json!({"id":id}), false)
            .await
            .unwrap();
        let outcome = framestudio::provider::generate(&job["snapshot"], &[], true).await;
        assert_eq!(outcome.usage["not_sent"], true);
        assert_eq!(outcome.cost, Some(0));
        assert!(outcome.error.unwrap().contains("kein Schlüsselbundzugriff"));
        assert_eq!(
            bridge
                .call("job.get", json!({"id":id}), false)
                .await
                .unwrap()["status"],
            "queued"
        );
        bridge
            .call("job.cancel", json!({"id":id}), false)
            .await
            .unwrap();
    }
    // Core-created database/journal files contain neither synthetic secret.
    for entry in std::fs::read_dir(dir.path()).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            let bytes = std::fs::read(path).unwrap();
            for value in [first, replacement] {
                assert!(
                    !bytes
                        .windows(value.len())
                        .any(|window| window == value.as_bytes())
                );
            }
        }
    }
}
