use base64::{Engine, engine::general_purpose::STANDARD};
use framestudio::core::Core;
use image::{DynamicImage, ImageFormat};
use serde_json::{Value, json};
use std::{fs, io::Cursor};
fn image_data() -> String {
    let mut data = Cursor::new(Vec::new());
    DynamicImage::new_rgb8(64, 48)
        .write_to(&mut data, ImageFormat::Png)
        .unwrap();
    STANDARD.encode(data.into_inner())
}
async fn setup() -> (tempfile::TempDir, Core, Value) {
    let root = tempfile::tempdir().unwrap();
    let core = Core::open(root.path().join("data")).unwrap();
    let project = core
        .call(
            "project.save",
            json!({"name":"Synthetische Abnahme","description":"Nur lokale Prüfdaten"}),
            false,
        )
        .await
        .unwrap();
    (root, core, project)
}
fn request(project: &Value, key: &str) -> Value {
    json!({"project_id":project["id"],"description":"Synthetische Testfigur","provider":"test","model":"deterministic-v1","aspect_ratio":"1:1","resolution":"1K","count":1,"idempotency_key":key,"blocks":{},"reference_ids":[]})
}
#[tokio::test]
async fn persistent_three_variants_character_and_snapshots() {
    let (_root, core, project) = setup().await;
    let state = core.call("state", json!({}), false).await.unwrap();
    assert_eq!(state["styles"].as_array().unwrap().len(), 11);
    let style = state["styles"][0].clone();
    let reference = core
        .call(
            "asset.import",
            json!({"project_id":project["id"],"name":"Referenz","data_base64":image_data()}),
            false,
        )
        .await
        .unwrap();
    let character=core.call("character.save",json!({"project_id":project["id"],"name":"Prüffigur","description":"Gelber Mantel","reference_ids":[reference["id"]],"approved":true}),false).await.unwrap();
    let mut req = request(&project, "three");
    req["style_id"] = style["id"].clone();
    req["character_id"] = character["id"].clone();
    req["variants"] = json!([{"pose":"stehen"},{"pose":"sitzen"},{"pose":"winken"}]);
    let jobs = core.call("job.submit", req.clone(), false).await.unwrap();
    assert_eq!(jobs["jobs"].as_array().unwrap().len(), 3);
    let first_snapshot = jobs["jobs"][0]["snapshot"].clone();
    assert!(
        first_snapshot["prompt"]
            .as_str()
            .unwrap()
            .contains("stehen")
    );
    assert_eq!(first_snapshot["references"][0]["id"], reference["id"]);
    core.call(
        "style.save",
        json!({"id":style["id"],"name":"Geändert","blocks":{"medium":"Öl"}}),
        false,
    )
    .await
    .unwrap();
    assert_eq!(
        core.call("job.get", json!({"id":jobs["jobs"][0]["id"]}), false)
            .await
            .unwrap()["snapshot"],
        first_snapshot
    );
    let repeated = core.call("job.submit", req, false).await.unwrap();
    assert_eq!(repeated["jobs"][0]["id"], jobs["jobs"][0]["id"]);
    for _ in 0..3 {
        assert!(core.work_once().await.unwrap());
    }
    assert!(!core.work_once().await.unwrap());
    let reopened = Core::open(core.root().to_owned()).unwrap();
    let assets = reopened
        .call("asset.list", json!({"project_id":project["id"]}), false)
        .await
        .unwrap();
    assert_eq!(assets["total"], 4);
    for asset in assets["items"].as_array().unwrap() {
        let read = reopened
            .call(
                "asset.read",
                json!({"id":asset["id"],"thumbnail":true}),
                false,
            )
            .await
            .unwrap();
        assert!(!read["data_base64"].as_str().unwrap().is_empty());
    }
    let costs = reopened
        .call("cost.project", json!({"project_id":project["id"]}), false)
        .await
        .unwrap();
    assert_eq!(costs["calculated_microusd"], 0);
    assert_eq!(costs["unknown_count"], 0);
}
#[tokio::test]
async fn idempotency_rejects_payload_reuse() {
    let (_root, core, p) = setup().await;
    let mut req = request(&p, "same");
    core.call("job.submit", req.clone(), false).await.unwrap();
    req["description"] = json!("Anderes Motiv");
    assert!(core.call("job.submit", req, false).await.is_err());
}
#[tokio::test]
async fn cancelled_before_send_costs_zero() {
    let (_root, core, p) = setup().await;
    let j = core
        .call("job.submit", request(&p, "cancel"), false)
        .await
        .unwrap();
    let result = core
        .call("job.cancel", json!({"id":j["jobs"][0]["id"]}), false)
        .await
        .unwrap();
    assert_eq!(result["status"], "cancelled");
    assert_eq!(result["actual_microusd"], 0);
    assert!(!core.work_once().await.unwrap());
}
#[tokio::test]
async fn budget_reservation_is_atomic_across_connections() {
    let (_root, core, p) = setup().await;
    core.call(
        "settings.save",
        json!({"project_limit_microusd":60000}),
        false,
    )
    .await
    .unwrap();
    let mut req = request(&p, "budget1");
    req["provider"] = json!("gemini");
    req["model"] = json!("gemini-nano-banana-2.1");
    let mut second = req.clone();
    second["idempotency_key"] = json!("budget2");
    let c = core.clone();
    let a = tokio::spawn(async move { c.call("job.submit", req, false).await });
    let c = core.clone();
    let b = tokio::spawn(async move { c.call("job.submit", second, false).await });
    let results = (a.await.unwrap(), b.await.unwrap());
    assert_ne!(results.0.is_ok(), results.1.is_ok());
}
#[tokio::test]
async fn automatic_paid_spend_requires_local_approval() {
    let (_root, core, p) = setup().await;
    let mut req = request(&p, "auto");
    req["provider"] = json!("gemini");
    req["model"] = json!("gemini-nano-banana-2.1");
    assert!(core.call("job.submit", req, true).await.is_err());
    assert!(
        core.call("settings.save", json!({"automation_approved":true}), true)
            .await
            .is_err()
    );
}
#[tokio::test]
async fn restart_never_resends_inflight() {
    let (_root, core, p) = setup().await;
    let j = core
        .call("job.submit", request(&p, "restart"), false)
        .await
        .unwrap();
    let id = j["jobs"][0]["id"].as_str().unwrap();
    let db = rusqlite::Connection::open(core.root().join("framestudio.sqlite3")).unwrap();
    let mut data = j["jobs"][0].clone();
    data["status"] = json!("running");
    db.execute(
        "UPDATE jobs SET status='running',data=?1 WHERE id=?2",
        rusqlite::params![data.to_string(), id],
    )
    .unwrap();
    assert!(!core.work_once().await.unwrap());
    let recovered = core.call("job.get", json!({"id":id}), false).await.unwrap();
    assert_eq!(recovered["status"], "unknown");
    assert_eq!(recovered["cost_status"], "unknown");
}
#[tokio::test]
async fn import_guards_and_project_ownership() {
    let (_root, core, p) = setup().await;
    assert!(core.call("asset.import",json!({"project_id":p["id"],"name":"evil.png","data_base64":STANDARD.encode(b"<svg onload='evil'>")}),false).await.is_err());
    let a = core
        .call(
            "asset.import",
            json!({"project_id":p["id"],"name":"ok","data_base64":image_data()}),
            false,
        )
        .await
        .unwrap();
    let p2 = core
        .call("project.save", json!({"name":"Zweitprojekt"}), false)
        .await
        .unwrap();
    let mut req = request(&p2, "cross");
    req["reference_ids"] = json!([a["id"]]);
    assert!(core.call("job.submit", req, false).await.is_err());
}
#[tokio::test]
async fn missing_files_are_reported() {
    let (_root, core, p) = setup().await;
    let asset = core
        .call(
            "asset.import",
            json!({"project_id":p["id"],"name":"reference","data_base64":image_data()}),
            false,
        )
        .await
        .unwrap();
    fs::remove_file(core.root().join(asset["path"].as_str().unwrap())).unwrap();
    assert!(
        core.call("asset.read", json!({"id":asset["id"]}), false)
            .await
            .is_err()
    );
}
#[test]
fn atomic_write_never_overwrites() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("same.png");
    framestudio::core::atomic_write(&path, b"one").unwrap();
    assert!(framestudio::core::atomic_write(&path, b"two").is_err());
    assert_eq!(fs::read(path).unwrap(), b"one");
}
#[cfg(unix)]
#[test]
fn symlink_and_traversal_rejected() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("original"), b"test").unwrap();
    std::os::unix::fs::symlink(root.path().join("original"), root.path().join("link")).unwrap();
    assert!(framestudio::core::checked_path(root.path(), "../evil").is_err());
    assert!(framestudio::core::checked_path(root.path(), "link").is_err());
}
#[tokio::test]
async fn concrete_variant_limits() {
    let (_root, core, p) = setup().await;
    let mut req = request(&p, "many");
    req["variants"] = json!(vec![json!({"pose":"stehen"}); 33]);
    assert!(core.call("job.submit", req, false).await.is_err());
}
#[tokio::test]
async fn project_duplicate_preserves_originals() {
    let (_root, core, p) = setup().await;
    let a = core
        .call(
            "asset.import",
            json!({"project_id":p["id"],"name":"ref","data_base64":image_data()}),
            false,
        )
        .await
        .unwrap();
    let copy = core
        .call("project.duplicate", json!({"id":p["id"]}), false)
        .await
        .unwrap();
    assert_ne!(copy["id"], p["id"]);
    assert_eq!(
        copy["name"],
        format!("{} · Kopie", p["name"].as_str().unwrap())
    );
    let persisted = core
        .call("project.get", json!({"id":copy["id"]}), false)
        .await
        .unwrap();
    assert_eq!(persisted, copy);
    core.call(
        "project.save",
        json!({"id":copy["id"], "name":"Titel · Wiederhergestellt"}),
        false,
    )
    .await
    .unwrap();
    let second = core
        .call("project.duplicate", json!({"id":copy["id"]}), false)
        .await
        .unwrap();
    assert_eq!(second["name"], "Titel · Wiederhergestellt · Kopie");

    let assets = core
        .call("asset.list", json!({"project_id":copy["id"]}), false)
        .await
        .unwrap();
    assert_eq!(assets["total"], 1);
    assert_ne!(assets["items"][0]["id"], a["id"]);
    assert!(
        core.call("asset.read", json!({"id":a["id"]}), false)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn variant_estimate_matches_total_reservation() {
    let (_root, core, p) = setup().await;
    let mut req = request(&p, "variant-cost");
    req["provider"] = json!("gemini");
    req["model"] = json!("gemini-nano-banana-2.1");
    req["variants"] = json!([{"pose":"stehen"},{"pose":"auf einem sehr langen Weg aufrecht und aufmerksam gehen"}]);
    let estimate = core
        .call("cost.estimate", req.clone(), false)
        .await
        .unwrap();
    let jobs = core.call("job.submit", req, false).await.unwrap();
    let total: i64 = jobs["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|j| j["reserved_microusd"].as_i64().unwrap())
        .sum();
    assert_eq!(estimate["total_microusd"], total);
}
#[test]
fn newer_database_version_is_not_downgraded() {
    let root = tempfile::tempdir().unwrap();
    let core = Core::open(root.path().join("data")).unwrap();
    let db = rusqlite::Connection::open(core.root().join("framestudio.sqlite3")).unwrap();
    db.execute_batch("PRAGMA user_version=999;").unwrap();
    assert!(Core::open(core.root().to_owned()).is_err());
    let v: i64 = db
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, 999);
}
#[tokio::test]
async fn export_queue_survives_restart_and_restore_preserves_history() {
    let (_root, core, p) = setup().await;
    core.call("job.submit", request(&p, "export-image"), false)
        .await
        .unwrap();
    assert!(core.work_once().await.unwrap());
    let job=core.call("export.submit",json!({"project_id":p["id"],"format":"zip","include_metadata":true,"idempotency_key":"backup"}),false).await.unwrap();
    assert_eq!(job["status"], "queued");
    let reopened = Core::open(core.root().to_owned()).unwrap();
    assert!(reopened.work_once().await.unwrap());
    let complete = reopened
        .call("export.get", json!({"id":job["id"]}), false)
        .await
        .unwrap();
    assert_eq!(complete["status"], "completed", "{complete}");
    let file = reopened
        .call("export.read", json!({"id":complete["result"]["id"]}), false)
        .await
        .unwrap();
    let restored = reopened
        .call(
            "project.restore",
            json!({"data_base64":file["data_base64"]}),
            false,
        )
        .await
        .unwrap();
    assert_ne!(restored["id"], p["id"]);
    let assets = reopened
        .call("asset.list", json!({"project_id":restored["id"]}), false)
        .await
        .unwrap();
    assert_eq!(assets["total"], 1);
    assert!(!reopened.work_once().await.unwrap());
    let costs = reopened
        .call("cost.project", json!({"project_id":restored["id"]}), false)
        .await
        .unwrap();
    assert_eq!(costs["calculated_microusd"], 0);
}
#[tokio::test]
async fn unsupported_parameters_fail_explicitly() {
    let (_root, core, p) = setup().await;
    let mut req = request(&p, "parameter");
    req["unknown_parameter"] = json!(true);
    assert!(
        core.call("job.submit", req, false)
            .await
            .unwrap_err()
            .to_string()
            .contains("Nicht unterstützter")
    );
}

#[tokio::test]
async fn another_open_does_not_recover_a_live_worker() {
    use fs2::FileExt;
    let (_root, core, p) = setup().await;
    let j = core
        .call("job.submit", request(&p, "live-worker"), false)
        .await
        .unwrap();
    let id = j["jobs"][0]["id"].as_str().unwrap();
    let db = rusqlite::Connection::open(core.root().join("framestudio.sqlite3")).unwrap();
    let mut data = j["jobs"][0].clone();
    data["status"] = json!("running");
    db.execute(
        "UPDATE jobs SET status='running',data=?1 WHERE id=?2",
        rusqlite::params![data.to_string(), id],
    )
    .unwrap();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(core.root().join("worker.lock"))
        .unwrap();
    lock.lock_exclusive().unwrap();
    let other = Core::open(core.root().to_owned()).unwrap();
    assert!(!other.work_once().await.unwrap());
    assert_eq!(
        other
            .call("job.get", json!({"id":id}), false)
            .await
            .unwrap()["status"],
        "running"
    );
    FileExt::unlock(&lock).unwrap();
    assert!(!other.work_once().await.unwrap());
    assert_eq!(
        other
            .call("job.get", json!({"id":id}), false)
            .await
            .unwrap()["status"],
        "unknown"
    );
}
#[tokio::test]
async fn malformed_blocks_are_errors_not_panics() {
    let (_root, core, p) = setup().await;
    let mut req = request(&p, "malformed");
    req["blocks"] = json!("bad");
    req["variants"] = json!([{"pose":"stehen"}]);
    assert!(core.call("job.submit", req, false).await.is_err());
}
#[tokio::test]
async fn image_write_failure_preserves_received_cost_and_prevents_retry() {
    let (_root, core, p) = setup().await;
    let j = core
        .call("job.submit", request(&p, "write-failure"), false)
        .await
        .unwrap();
    fs::remove_dir(core.root().join("thumbnails")).unwrap();
    fs::write(core.root().join("thumbnails"), b"synthetic write blocker").unwrap();
    assert!(core.work_once().await.is_err());
    let job = core
        .call("job.get", json!({"id":j["jobs"][0]["id"]}), false)
        .await
        .unwrap();
    assert_eq!(job["status"], "unknown");
    assert_eq!(job["actual_microusd"], 0);
    assert_eq!(job["cost_status"], "usage_calculated");
    assert!(!core.work_once().await.unwrap());
}
#[tokio::test]
async fn warning_threshold_is_visible_before_start() {
    let (_root, core, p) = setup().await;
    core.call("settings.save", json!({"warning_microusd":1}), false)
        .await
        .unwrap();
    let mut req = request(&p, "warning");
    req["provider"] = json!("gemini");
    req["model"] = json!("gemini-nano-banana-2.1");
    let e = core.call("cost.estimate", req, false).await.unwrap();
    assert!(
        e["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w.as_str().unwrap().contains("Warnschwelle"))
    );
}

#[tokio::test]
async fn export_preview_uses_native_naming_and_checks_ownership() {
    let (_root, core, p) = setup().await;
    let p = core
        .call(
            "project.save",
            json!({"id":p["id"],"name":"Synthetisches Prüfprojekt"}),
            false,
        )
        .await
        .unwrap();
    let asset = core
        .call(
            "asset.import",
            json!({"project_id":p["id"],"name":"Referenz","data_base64":image_data()}),
            false,
        )
        .await
        .unwrap();
    let preview=core.call("export.preview",json!({"project_id":p["id"],"asset_id":asset["id"],"format":"png","naming_template":"{projekt}_{nummer:04}"}),false).await.unwrap();
    assert_eq!(preview["filename"], "Synthetisches Prüfprojekt_0001.png");
    let other = core
        .call("project.save", json!({"name":"Anderes Projekt"}), false)
        .await
        .unwrap();
    assert!(
        core.call(
            "export.preview",
            json!({"project_id":other["id"],"asset_id":asset["id"],"format":"png"}),
            false
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn structured_prompt_sections_preserve_sources_references_and_history() {
    let (dir, core, project) = setup().await;
    let reference = core.call("asset.import", json!({"project_id":project["id"],"data_base64":image_data(),"description":"Reference"}), false).await.unwrap();
    let character = core.call("character.save", json!({"project_id":project["id"],"name":"Pilot","description":"Brauner Bart","clothing":"Grüner Anzug","reference_ids":[reference["id"]],"approved":true}), false).await.unwrap();
    let mut args = request(&project, "structured");
    args["description"] = json!("Pilot winkt");
    args["character_id"] = character["id"].clone();
    args["blocks"] = json!({"scene":"Heller Hangar","character":"Zweiter Pilot mit Brille","composition":"Beide ganz im Bild","pose":"Rechter Arm oben","mouth":"O"});
    let preview = core
        .call("prompt.preview", args.clone(), false)
        .await
        .unwrap();
    let prompt = preview["prompt"].as_str().unwrap();
    let scene = prompt.find("[Gesamte Szene]").unwrap();
    let figure = prompt.find("[Charakter]").unwrap();
    let instruction = prompt.find("[Einzelanweisung]").unwrap();
    assert!(scene < figure && figure < instruction);
    assert!(prompt[scene..figure].contains("Heller Hangar"));
    assert!(prompt[figure..instruction].contains("Brauner Bart"));
    assert!(prompt[figure..instruction].contains("Zweiter Pilot mit Brille"));
    assert!(!prompt[figure..instruction].contains("Pilot winkt"));
    assert!(prompt[instruction..].contains("Pilot winkt"));
    assert_eq!(prompt.matches("Zweiter Pilot mit Brille").count(), 1);
    args["variants"] = json!([{"view":"frontal","pose":"gehen"},{"view":"Profil","pose":"sitzen"}]);
    let jobs = core.call("job.submit", args, false).await.unwrap()["jobs"]
        .as_array()
        .unwrap()
        .clone();
    for job in &jobs {
        assert_eq!(job["snapshot"]["blocks"]["scene"], "Heller Hangar");
        assert_eq!(
            job["snapshot"]["blocks"]["character"],
            "Zweiter Pilot mit Brille"
        );
        assert_eq!(job["snapshot"]["references"][0]["id"], reference["id"]);
    }
    core.call("character.save",json!({"id":character["id"],"project_id":project["id"],"name":"Pilot","description":"Geänderte Figur"}),false).await.unwrap();
    core.call(
        "project.save",
        json!({"id":project["id"],"name":"Edited","description":"Anderer Projektkontext"}),
        false,
    )
    .await
    .unwrap();
    let reopened = Core::open(dir.path().join("data")).unwrap();
    for job in jobs {
        assert_eq!(
            reopened
                .call("job.get", json!({"id":job["id"]}), false)
                .await
                .unwrap()["snapshot"],
            job["snapshot"]
        );
    }
}
