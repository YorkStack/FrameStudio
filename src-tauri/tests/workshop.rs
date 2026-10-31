use framestudio::{assistant, core::Core};
use serde_json::{Value, json};
async fn call(c: &Core, op: &str, a: Value) -> Value {
    c.call(op, a, false).await.unwrap()
}
#[tokio::test]
async fn template_trash_portrait_and_sbom_survive_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let c = Core::open(dir.path().into()).unwrap();
    let p = call(&c, "project.save", json!({"name":"Workspace regression"})).await;
    let id = &p["id"];
    let template = json!({"scene":"Werkstatt","character":"Roboter","instruction":"Winken"});
    call(
        &c,
        "project.prompt.save",
        json!({"id":id,"prompt_template":template}),
    )
    .await;
    assert!(
        c.call(
            "project.prompt.save",
            json!({"id":id,"prompt_template":{"unknown":"x"}}),
            false
        )
        .await
        .is_err()
    );
    let ch = call(
        &c,
        "character.save",
        json!({"project_id":id,"name":"Robot","reference_ids":[],"approved":false}),
    )
    .await;
    let req = json!({"project_id":id,"character_id":ch["id"],"description":"Winken","blocks":{"scene":"Werkstatt","character":"Roboter"},"provider":"test","model":"deterministic-v1","aspect_ratio":"1:1","count":1,"idempotency_key":"workshop"});
    let j = call(&c, "job.submit", req).await["jobs"][0].clone();
    assert!(
        c.call("project.trash", json!({"id":id}), false)
            .await
            .is_err()
    );
    assert!(c.work_once().await.unwrap());
    let job = call(&c, "job.get", json!({"id":j["id"]})).await;
    assert_eq!(job["status"], "completed");
    let chars = call(&c, "character.list", json!({"project_id":id})).await;
    assert_eq!(
        chars[0]["portrait_asset_id"],
        call(&c, "asset.list", json!({"project_id":id})).await["items"][0]["id"]
    );
    assert_eq!(chars[0]["portrait_automatic"], true);
    let cost = call(&c, "cost.project", json!({"project_id":id})).await;
    call(&c, "project.trash", json!({"id":id})).await;
    assert!(
        call(&c, "project.list", json!({}))
            .await
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        call(&c, "project.trash.list", json!({}))
            .await
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        call(&c, "cost.project", json!({"project_id":id})).await,
        cost
    );
    assert!(c.call("job.submit",json!({"project_id":id,"description":"must not start","provider":"test","model":"deterministic-v1","idempotency_key":"deleted"}),false).await.is_err());
    call(&c, "project.untrash", json!({"id":id})).await;
    let reopened = Core::open(dir.path().into()).unwrap();
    assert_eq!(
        call(&reopened, "project.get", json!({"id":id})).await["prompt_template"],
        template
    );
    let package = call(
        &c,
        "export.create",
        json!({"project_id":id,"format":"zip","include_metadata":true}),
    )
    .await;
    let bytes = call(&c, "export.read", json!({"id":package["id"]})).await;
    let restored = call(
        &c,
        "project.restore",
        json!({"data_base64":bytes["data_base64"]}),
    )
    .await;
    assert_eq!(restored["prompt_template"], template);
    let sbom = call(&c, "app.sbom", json!({})).await;
    assert_eq!(sbom["bomFormat"], "CycloneDX");
    assert_eq!(
        sbom["metadata"]["component"]["version"],
        env!("CARGO_PKG_VERSION")
    );
    assert!(sbom["components"].as_array().unwrap().len() > 600);
    let export = call(&c, "app.sbom.export", json!({})).await;
    let data: Value =
        serde_json::from_slice(&std::fs::read(export["path"].as_str().unwrap()).unwrap()).unwrap();
    assert_eq!(data, sbom);
}
#[tokio::test]
async fn structured_assistant_has_editable_sections_and_frozen_target() {
    let dir = tempfile::tempdir().unwrap();
    let c = Core::open(dir.path().into()).unwrap();
    let p = call(&c, "project.save", json!({"name":"Assistant"})).await;
    let parts = json!({"scene":"Space","character":"Robot","instruction":"Wave"});
    let req = json!({"project_id":p["id"],"provider":"test","model":assistant::TEST_MODEL,"target":"all","text":parts.to_string(),"idempotency_key":"sections"});
    let j = call(&c, "assistant.submit", req).await["jobs"][0].clone();
    assert!(c.work_once().await.unwrap());
    let result = call(&c, "job.get", json!({"id":j["id"]})).await;
    assert_eq!(result["status"], "completed");
    assert_eq!(result["result_sections"]["scene"], "Space");
    assert_eq!(result["result_sections"]["character"], "Robot");
    assert!(
        result["result_sections"]["instruction"]
            .as_str()
            .unwrap()
            .contains("Wave")
    );
    assert!(assistant::parameters(&json!({"text":"x","target":"secret"})).is_err());
    assert!(call(&c, "project.get", json!({"id":p["id"]})).await["prompt_template"].is_null());
}

#[tokio::test]
async fn language_is_validated_and_persists_without_changing_user_content() {
    let dir = tempfile::tempdir().unwrap();
    let c = Core::open(dir.path().into()).unwrap();
    let project = call(
        &c,
        "project.save",
        json!({"name":"Mein Original","description":"Unveränderter Text"}),
    )
    .await;
    call(&c, "settings.save", json!({"language":"en"})).await;
    assert!(
        c.call("settings.save", json!({"language":"fr"}), false)
            .await
            .is_err()
    );
    drop(c);
    let c = Core::open(dir.path().into()).unwrap();
    let state = call(&c, "state", json!({})).await;
    assert_eq!(state["settings"]["language"], "en");
    assert_eq!(
        call(&c, "project.get", json!({"id":project["id"]})).await["description"],
        "Unveränderter Text"
    );
}
