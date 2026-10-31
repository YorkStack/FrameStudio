use base64::{Engine, engine::general_purpose::STANDARD};
use framestudio::core::Core;
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    fs,
    io::{Cursor, Read, Write},
};

async fn setup() -> (tempfile::TempDir, Core, Vec<Value>) {
    let directory = tempfile::tempdir().unwrap();
    let core = Core::open(directory.path().into()).unwrap();
    let styles = core
        .call("style.list", json!({}), false)
        .await
        .unwrap()
        .as_array()
        .unwrap()
        .clone();
    (directory, core, styles)
}
async fn prepare(core: &Core, style: &Value) -> Value {
    core.call("style.preview.prepare", json!({"id":style["id"]}), false)
        .await
        .unwrap()
}
fn request(prepared: &Value) -> Value {
    json!({"project_id":prepared["project"]["id"],"style_id":prepared["style"]["id"],"description":prepared["description"],"blocks":prepared["blocks"],"provider":"test","model":"deterministic-v1","aspect_ratio":"1:1","resolution":"1K","count":1,"idempotency_key":uuid::Uuid::new_v4().to_string()})
}
async fn generate(core: &Core, prepared: &Value) -> (Value, Value) {
    let job = core
        .call("job.submit", request(prepared), false)
        .await
        .unwrap()["jobs"][0]
        .clone();
    assert!(core.work_once().await.unwrap());
    let job = core
        .call("job.get", json!({"id":job["id"]}), false)
        .await
        .unwrap();
    assert_eq!(job["status"], "completed");
    let assets = core
        .call(
            "asset.list",
            json!({"project_id":prepared["project"]["id"],"style_id":prepared["style"]["id"]}),
            false,
        )
        .await
        .unwrap();
    (job, assets["items"][0].clone())
}
async fn adopt(core: &Core, style: &Value, asset: &Value) -> Value {
    core.call(
        "style.preview.set",
        json!({"id":style["id"],"asset_id":asset["id"]}),
        false,
    )
    .await
    .unwrap()
}
async fn archive(core: &Core, project: &Value) -> Value {
    let output = core
        .call(
            "export.create",
            json!({"project_id":project["id"],"format":"zip","include_metadata":true}),
            false,
        )
        .await
        .unwrap();
    core.call("export.read", json!({"id":output["id"]}), false)
        .await
        .unwrap()
}

#[tokio::test]
async fn all_prepared_styles_share_scene_but_keep_distinct_style_prompts_without_jobs() {
    let (_directory, core, styles) = setup().await;
    assert_eq!(styles.len(), 11);
    let mut prompts = HashSet::new();
    let mut projects = HashSet::new();
    for style in &styles {
        let prepared = prepare(&core, style).await;
        assert_eq!(prepared["style"]["blocks"], style["blocks"]);
        assert_eq!(prepared["description"], styles[0]["preview_prompt"]);
        assert_eq!(prepared["project"]["description"], "");
        assert_eq!(
            prepared["project"]["preferences"]["purpose"],
            "style_previews"
        );
        let preview = core
            .call("prompt.preview", request(&prepared), false)
            .await
            .unwrap();
        for value in style["blocks"].as_object().unwrap().values() {
            assert!(
                preview["prompt"]
                    .as_str()
                    .unwrap()
                    .contains(value.as_str().unwrap())
            );
        }
        assert!(preview["prompt"].as_str().unwrap().contains("Kaffeetasse"));
        prompts.insert(preview["prompt"].as_str().unwrap().to_owned());
        projects.insert(prepared["project"]["id"].as_str().unwrap().to_owned());
    }
    assert_eq!(prompts.len(), 11);
    assert_eq!(projects.len(), 1);
    assert!(
        core.call("job.list", json!({}), false)
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty()
    );
    let state = core.call("state", json!({}), false).await.unwrap();
    assert_eq!(
        state["styles"][0]["preview_prompt"],
        styles[0]["preview_prompt"]
    );
}

#[tokio::test]
async fn concurrent_prepare_creates_one_project_and_archived_project_is_preserved() {
    let (_directory, core, styles) = setup().await;
    let a = core.clone();
    let b = core.clone();
    let id_a = styles[0]["id"].clone();
    let id_b = styles[1]["id"].clone();
    let first = tokio::spawn(async move {
        a.call("style.preview.prepare", json!({"id":id_a}), false)
            .await
            .unwrap()
    });
    let second = tokio::spawn(async move {
        b.call("style.preview.prepare", json!({"id":id_b}), false)
            .await
            .unwrap()
    });
    let first = first.await.unwrap();
    let second = second.await.unwrap();
    assert_eq!(first["project"]["id"], second["project"]["id"]);
    let mut project = first["project"].clone();
    project["archived"] = json!(true);
    core.call("project.save", project.clone(), false)
        .await
        .unwrap();
    let next = prepare(&core, &styles[0]).await;
    assert_ne!(next["project"]["id"], project["id"]);
    assert_eq!(
        core.call("project.get", json!({"id":project["id"]}), false)
            .await
            .unwrap()["archived"],
        true
    );
}

#[tokio::test]
async fn adoption_restart_versioning_duplicate_and_clear_preserve_history() {
    let (directory, core, styles) = setup().await;
    let prepared = prepare(&core, &styles[0]).await;
    let (job, asset) = generate(&core, &prepared).await;
    let original = fs::read(core.root().join(asset["path"].as_str().unwrap())).unwrap();
    let adopted = adopt(&core, &styles[0], &asset).await;
    assert_eq!(adopted["version"], 1);
    assert_eq!(adopted["preview_style_version"], 1);
    assert_eq!(adopted["preview_provider"], "test");
    let restarted = Core::open(directory.path().into()).unwrap();
    let saved = restarted
        .call("style.list", json!({}), false)
        .await
        .unwrap();
    assert_eq!(saved[0]["preview_asset_id"], asset["id"]);
    let mut changed = adopted.clone();
    changed["preview_prompt"] = json!("Synthetische neue Motivbeschreibung");
    let changed = restarted.call("style.save", changed, false).await.unwrap();
    assert_eq!(changed["version"], 2);
    assert_eq!(changed["preview_style_version"], 1);
    assert_eq!(
        changed["versions"][0]["preview_prompt"],
        styles[0]["preview_prompt"]
    );
    assert_eq!(
        prepare(&restarted, &changed).await["description"],
        changed["preview_prompt"]
    );
    let custom_preview = restarted
        .call(
            "prompt.preview",
            request(&prepare(&restarted, &changed).await),
            false,
        )
        .await
        .unwrap();
    assert!(
        custom_preview["prompt"]
            .as_str()
            .unwrap()
            .contains("Synthetische neue Motivbeschreibung")
    );
    assert!(
        !custom_preview["prompt"]
            .as_str()
            .unwrap()
            .contains("Roboter")
    );
    assert_eq!(
        restarted
            .call("job.get", json!({"id":job["id"]}), false)
            .await
            .unwrap()["snapshot"],
        job["snapshot"]
    );
    let duplicate = restarted
        .call("style.duplicate", json!({"id":changed["id"]}), false)
        .await
        .unwrap();
    assert_eq!(duplicate["preview_prompt"], changed["preview_prompt"]);
    assert!(duplicate.get("preview_asset_id").is_none());
    let cleared = restarted
        .call(
            "style.preview.set",
            json!({"id":changed["id"],"asset_id":null}),
            false,
        )
        .await
        .unwrap();
    assert_eq!(cleared["version"], 2);
    assert!(cleared.get("preview_asset_id").is_none());
    assert_eq!(
        fs::read(core.root().join(asset["path"].as_str().unwrap())).unwrap(),
        original
    );
}

#[tokio::test]
async fn preview_input_and_asset_provenance_are_validated() {
    let (_directory, core, styles) = setup().await;
    for value in [
        Value::Null,
        json!(false),
        json!(""),
        json!("x".repeat(12001)),
    ] {
        let mut invalid = styles[0].clone();
        invalid["preview_prompt"] = value;
        assert!(core.call("style.save", invalid, false).await.is_err());
    }
    for args in [
        json!({"id":styles[0]["id"]}),
        json!({"id":styles[0]["id"],"asset_id":false}),
        json!({"id":styles[0]["id"],"asset_id":"missing"}),
    ] {
        assert!(core.call("style.preview.set", args, false).await.is_err());
    }
    let prepared = prepare(&core, &styles[0]).await;
    let (_, asset) = generate(&core, &prepared).await;
    assert!(
        core.call(
            "style.preview.set",
            json!({"id":styles[1]["id"],"asset_id":asset["id"]}),
            false
        )
        .await
        .is_err()
    );
    fs::remove_file(core.root().join(asset["thumbnail_path"].as_str().unwrap())).unwrap();
    assert!(
        core.call(
            "style.preview.set",
            json!({"id":styles[0]["id"],"asset_id":asset["id"]}),
            false
        )
        .await
        .is_err()
    );
    assert!(
        core.call("style.list", json!({}), false).await.unwrap()[0]
            .get("preview_asset_id")
            .is_none()
    );
}

#[tokio::test]
async fn style_filter_applies_before_pagination_and_rejects_bad_queries() {
    let (_directory, core, styles) = setup().await;
    let prepared = prepare(&core, &styles[0]).await;
    let (_, first) = generate(&core, &prepared).await;
    let other = prepare(&core, &styles[1]).await;
    generate(&core, &other).await;
    let query = json!({"project_id":prepared["project"]["id"],"style_id":styles[0]["id"],"offset":0,"limit":1});
    let result = core.call("asset.list", query.clone(), false).await.unwrap();
    assert_eq!(result["total"], 1);
    assert_eq!(result["items"][0]["id"], first["id"]);
    for value in [Value::Null, json!(false), json!("unknown-style")] {
        let mut invalid = query.clone();
        invalid["style_id"] = value;
        assert!(core.call("asset.list", invalid, false).await.is_err());
    }
}

#[tokio::test]
async fn backup_remaps_preview_and_external_preview_is_omitted() {
    let (_directory, core, styles) = setup().await;
    let prepared = prepare(&core, &styles[0]).await;
    let (_, asset) = generate(&core, &prepared).await;
    adopt(&core, &styles[0], &asset).await;
    let backup = archive(&core, &prepared["project"]).await;
    let project = core
        .call(
            "project.restore",
            json!({"data_base64":backup["data_base64"]}),
            false,
        )
        .await
        .unwrap();
    let restored_assets = core
        .call("asset.list", json!({"project_id":project["id"]}), false)
        .await
        .unwrap();
    let restored_asset = core
        .call(
            "asset.get",
            json!({"id":restored_assets["items"][0]["id"]}),
            false,
        )
        .await
        .unwrap();
    let all_styles = core.call("style.list", json!({}), false).await.unwrap();
    let restored_style = all_styles
        .as_array()
        .unwrap()
        .iter()
        .find(|style| style["preview_asset_id"] == restored_asset["id"])
        .unwrap();
    assert_ne!(restored_style["id"], styles[0]["id"]);
    assert_eq!(
        restored_asset["snapshot"]["style"]["id"],
        restored_style["id"]
    );
    assert_eq!(restored_style["preview_provider"], "test");
    assert_eq!(restored_style["preview_style_version"], 1);
    let other = core
        .call(
            "project.save",
            json!({"name":"Unrelated project","style_id":styles[0]["id"]}),
            false,
        )
        .await
        .unwrap();
    let backup = archive(&core, &other).await;
    let bytes = STANDARD
        .decode(backup["data_base64"].as_str().unwrap())
        .unwrap();
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut manifest = String::new();
    zip.by_name("manifest.json")
        .unwrap()
        .read_to_string(&mut manifest)
        .unwrap();
    let manifest: Value = serde_json::from_str(&manifest).unwrap();
    assert!(manifest["styles"][0].get("preview_asset_id").is_none());
    assert!(manifest["styles"][0]["preview_prompt"].is_string());
}

#[tokio::test]
async fn malformed_import_preview_and_automated_spend_are_rejected() {
    let (_directory, core, styles) = setup().await;
    let mut style = styles[0].clone();
    style["preview_asset_id"] = json!(false);
    let manifest = json!({"version":1,"application":"FrameStudio","project":{"id":uuid::Uuid::new_v4().to_string(),"name":"Malformed preview"},"styles":[style],"assets":[],"characters":[],"jobs":[]});
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file("manifest.json", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(serde_json::to_string(&manifest).unwrap().as_bytes())
        .unwrap();
    let bytes = zip.finish().unwrap().into_inner();
    assert!(
        core.call(
            "project.restore",
            json!({"data_base64":STANDARD.encode(bytes)}),
            false
        )
        .await
        .is_err()
    );
    let prepared = prepare(&core, &styles[0]).await;
    let mut paid = request(&prepared);
    paid["provider"] = json!("openai");
    paid["model"] = json!(framestudio::provider::openai::SUNBURST);
    assert!(
        core.call("job.submit", paid, true)
            .await
            .unwrap_err()
            .to_string()
            .contains("nicht lokal freigegeben")
    );
    assert!(
        core.call("job.list", json!({}), false)
            .await
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty()
    );
}
