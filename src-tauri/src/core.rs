//! The single local application core shared by desktop, REST and MCP.
use crate::{assistant, pricing, provider};
use anyhow::{Context, Result, anyhow, bail, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::Utc;
use fs2::FileExt;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{Cursor, Write},
    path::{Component, Path, PathBuf},
    time::Duration,
};
use uuid::Uuid;
mod series;
mod workspace;

#[derive(Clone)]
pub struct Core {
    root: PathBuf,
}
fn now() -> String {
    Utc::now().to_rfc3339()
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn new_id() -> String {
    Uuid::new_v4().to_string()
}
fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or("")
}
fn required<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    let s = text(value, key);
    if s.trim().is_empty() || s.len() > 32_000 {
        bail!("Feld {key} fehlt oder ist zu lang");
    }
    Ok(s)
}
fn array(value: &Value, key: &str) -> Vec<Value> {
    value[key].as_array().cloned().unwrap_or_default()
}
fn copy_fields(target: &mut Value, source: &Value, fields: &[&str]) {
    for field in fields {
        if let Some(v) = source.get(*field) {
            target[*field] = v.clone();
        }
    }
}

impl Core {
    pub fn open(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(&root).context("Datenverzeichnis konnte nicht erstellt werden")?;
        if fs::symlink_metadata(&root)?.file_type().is_symlink() {
            bail!("Datenverzeichnis darf kein symbolischer Link sein");
        }
        let root = root.canonicalize()?;
        private_dir(&root)?;
        for dir in ["assets", "thumbnails", "exports"] {
            let p = root.join(dir);
            fs::create_dir_all(&p)?;
            if fs::symlink_metadata(&p)?.file_type().is_symlink() {
                bail!("Datenpfad ist ein symbolischer Link");
            }
            private_dir(&p)?;
        }
        let core = Self { root };
        let mut conn = core.connect()?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > 1 {
            bail!("Diese Datenbank stammt aus einer neueren FrameStudio-Version");
        }
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;")?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS records(kind TEXT NOT NULL,id TEXT NOT NULL,project_id TEXT NOT NULL DEFAULT '',created_at TEXT NOT NULL,data TEXT NOT NULL,PRIMARY KEY(kind,id)); CREATE INDEX IF NOT EXISTS records_project ON records(kind,project_id,created_at); CREATE TABLE IF NOT EXISTS jobs(id TEXT PRIMARY KEY,project_id TEXT NOT NULL,status TEXT NOT NULL,idempotency_key TEXT NOT NULL UNIQUE,created_at TEXT NOT NULL,reserved INTEGER NOT NULL DEFAULT 0,actual INTEGER,cost_status TEXT NOT NULL,data TEXT NOT NULL); CREATE INDEX IF NOT EXISTS jobs_queue ON jobs(status,created_at); PRAGMA user_version=1;")?;
        if find(&tx, "settings", "global")?.is_none() {
            put(&tx, "settings", &default_settings())?;
        }
        for style in default_styles() {
            if find(&tx, "style", text(&style, "id"))?.is_none() {
                put(&tx, "style", &style)?;
            }
        }
        tx.commit()?;
        Ok(core)
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    fn connect(&self) -> Result<Connection> {
        let path = self.root.join("framestudio.sqlite3");
        if path.exists() && fs::symlink_metadata(&path)?.file_type().is_symlink() {
            bail!("Datenbank darf kein symbolischer Link sein");
        }
        let conn = Connection::open(path)?;
        conn.busy_timeout(Duration::from_secs(10))?;
        conn.execute_batch("PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL;")?;
        Ok(conn)
    }
    pub(crate) fn submit_series(&self, args: &Value, origin: Value) -> Result<Value> {
        series::submit(&mut self.connect()?, args, origin)
    }
    pub async fn call(&self, op: &str, args: Value, automated: bool) -> Result<Value> {
        if !args.is_object() {
            bail!("Argumente müssen ein JSON-Objekt sein");
        }
        if serde_json::to_vec(&args)?.len() > 90 * 1024 * 1024 {
            bail!("Anfrage zu groß");
        }
        if op == "pricing.refresh" {
            return self.refresh_prices(&args).await;
        }
        let mut conn = self.connect()?;
        match op {
            "state" => Ok(
                json!({"app_version":env!("CARGO_PKG_VERSION"),"projects":workspace::active_projects(&conn)?,"styles":list_styles(&conn)?,"characters":workspace::characters(&conn,None)?,"settings":settings(&conn)?,"models":provider::models(),"assistant_models":assistant::models(),"pricing_revision":digest(&serde_json::to_vec(&price_catalog(&conn)?)?),"key_status":crate::secrets::key_status(),"data_dir":self.root}),
            ),
            "pricing.catalog" => price_catalog(&conn),
            "assistant.models" => Ok(assistant::models()),
            "assistant.estimate" => estimate_request(&conn, &args, true),
            "assistant.submit" => submit_kind(&mut conn, &args, automated, true),
            "project.list" => Ok(json!(workspace::active_projects(&conn)?)),
            "project.prompt.save" => workspace::save_prompt(&conn, &args),
            "project.trash" | "project.untrash" | "project.trash.list" => {
                ensure!(!automated, "Papierkorb nur lokal verwalten");
                workspace::trash(&mut conn, op, &args)
            }
            "app.sbom.export" => {
                ensure!(!automated, "SBOM-Export nur lokal");
                let id = new_id();
                let directory = self.root.join("exports").join(&id);
                fs::create_dir(&directory)?;
                let name = format!("FrameStudio-{}-sbom.cdx.json", env!("CARGO_PKG_VERSION"));
                atomic_write(
                    &directory.join(&name),
                    include_bytes!("../resources/sbom.json"),
                )?;
                Ok(json!({"id":id,"path":directory.join(name)}))
            }
            "app.sbom" => Ok(serde_json::from_str(include_str!(
                "../resources/sbom.json"
            ))?),
            "project.get" => get(&conn, "project", required(&args, "id")?),
            "project.save" => {
                if automated && args.get("budget_microusd").is_some() {
                    bail!("Projektbudgets können nur lokal freigegeben werden");
                }
                save_project(&conn, &args)
            }
            "project.duplicate" => self.duplicate_project(&mut conn, &args),
            "style.list" => Ok(json!(list_styles(&conn)?)),
            "style.preview.prepare" => prepare_style_preview(&mut conn, &args),
            "style.preview.set" => self.set_style_preview(&mut conn, &args),
            "style.save" => save_style(&mut conn, &args),
            "style.duplicate" => {
                let mut s = get(&conn, "style", required(&args, "id")?)?;
                s["id"] = Value::Null;
                s["name"] = json!(format!("{} · Kopie", text(&s, "name")));
                save_style(&mut conn, &s)
            }
            "character.list" => Ok(json!(workspace::characters(
                &conn,
                args["project_id"].as_str()
            )?)),
            "character.save" => save_character(&conn, &args),
            "asset.import" => self.import_asset(&conn, &args),
            "asset.list" => list_assets(&conn, &args),
            "asset.update" => update_asset(&conn, &args),
            "asset.get" => get(&conn, "asset", required(&args, "id")?),
            "asset.read" => self.read_asset(&conn, &args),
            "prompt.preview" => {
                let snapshot = build_snapshot(&conn, &args)?;
                Ok(json!({"prompt":snapshot["prompt"],"snapshot":snapshot}))
            }
            "cost.estimate" => estimate_batch(&conn, &args),
            "job.submit" => submit(&mut conn, &args, automated),
            "series.submit" => self.submit_series(
                &args,
                json!({"channel":if automated {"automation"} else {"local"}}),
            ),
            "series.list" => Ok(json!(
                list(&conn, "series", args["project_id"].as_str())?
                    .into_iter()
                    .map(|record| series::view(&conn, record))
                    .collect::<Result<Vec<_>>>()?
            )),
            "series.quote" => series::quote(&conn, &args),
            "series.get" => series::view(&conn, get(&conn, "series", required(&args, "id")?)?),
            "series.approve" | "series.reject" => {
                if automated {
                    bail!(
                        "Serien dürfen nur lokal in FrameStudio freigegeben oder abgelehnt werden"
                    );
                }
                if op == "series.approve" {
                    series::approve(&mut conn, &args)
                } else {
                    series::cancel(&mut conn, &args, true)
                }
            }
            "series.cancel" => series::cancel(&mut conn, &args, false),
            "job.list" => Ok(json!(list_jobs(&conn, args["project_id"].as_str())?)),
            "job.get" => get_job(&conn, required(&args, "id")?),
            "job.cancel" => cancel_job(&conn, &args),
            "cost.project" => {
                get(&conn, "project", required(&args, "project_id")?)?;
                project_cost(&conn, required(&args, "project_id")?)
            }
            "settings.save" => save_settings(&conn, &args, automated),
            "settings.get" => settings(&conn),
            "export.preview" => export_preview(&conn, &args),
            "export.submit" => self.submit_export(&mut conn, &args),
            "export.get" => get(&conn, "export_job", required(&args, "id")?),
            "export.create" => self.export(&conn, &args),
            "export.read" => crate::exports::read_export(&self.root, required(&args, "id")?),
            "project.restore" => self.restore(&mut conn, &args),
            _ => bail!("Unbekannte Operation: {op}"),
        }
    }
    fn set_style_preview(&self, conn: &mut Connection, args: &Value) -> Result<Value> {
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut style = get(&tx, "style", required(args, "id")?)?;
        match args.get("asset_id") {
            Some(Value::Null) => {
                clear_style_preview(&mut style);
                style["preview_hidden"] = json!(true);
            }
            Some(Value::String(id)) => {
                let asset = get(&tx, "asset", id)?;
                apply_style_preview(&mut style, &asset)?;
                self.read_asset(&tx, &json!({"id":id}))?;
                self.read_asset(&tx, &json!({"id":id,"thumbnail":true}))?;
            }
            _ => bail!("Vorschaubild-ID muss Text oder ausdrücklich null sein"),
        }
        style["updated_at"] = json!(now());
        put(&tx, "style", &style)?;
        tx.commit()?;
        Ok(with_preview_prompt(style))
    }
    fn import_asset(&self, conn: &Connection, args: &Value) -> Result<Value> {
        let project = get(conn, "project", required(args, "project_id")?)?;
        let data = text(args, "data_base64");
        if data.is_empty() || data.len() > provider::MAX_IMAGE_BYTES * 4 / 3 + 8 {
            bail!("Importbild fehlt oder überschreitet 20 MiB");
        }
        let bytes = STANDARD
            .decode(text(args, "data_base64"))
            .map_err(|_| anyhow!("Ungültige Base64-Daten"))?;
        let (mut asset, _) = self.store_image(
            &bytes,
            text(&project, "id"),
            text(args, "description"),
            None,
        )?;
        asset["description"] = json!(
            args["description"]
                .as_str()
                .unwrap_or_else(|| text(args, "name"))
        );
        asset["status"] = json!("reference");
        asset["provider"] = json!("import");
        put(conn, "asset", &asset)?;
        Ok(asset)
    }
    fn store_image(
        &self,
        bytes: &[u8],
        project_id: &str,
        description: &str,
        snapshot: Option<&Value>,
    ) -> Result<(Value, Vec<PathBuf>)> {
        let (image, format) = provider::validate_image(bytes)?;
        let id = new_id();
        let ext = format.extensions_str()[0];
        let path = format!("assets/{id}.{ext}");
        let thumbnail_path = format!("thumbnails/{id}.png");
        let mut thumb = Cursor::new(Vec::new());
        image
            .thumbnail(320, 320)
            .write_to(&mut thumb, image::ImageFormat::Png)?;
        atomic_write(&self.root.join(&path), bytes)?;
        if let Err(e) = atomic_write(&self.root.join(&thumbnail_path), &thumb.into_inner()) {
            let _ = fs::remove_file(self.root.join(&path));
            return Err(e);
        }
        let mut asset = json!({"id":id,"project_id":project_id,"description":description,"created_at":now(),"path":path,"thumbnail_path":thumbnail_path,"width":image.width(),"height":image.height(),"format":ext,"mime":format.to_mime_type(),"sha256":digest(bytes),"tags":[],"favorite":false,"comment":"","status":"ready","cost_status":"unknown","cost_microusd":null});
        if let Some(snapshot) = snapshot {
            asset["snapshot"] = snapshot.clone();
            asset["prompt"] = snapshot["prompt"].clone();
            asset["parameters"] = snapshot["parameters"].clone();
            asset["reference_ids"] = json!(
                array(snapshot, "references")
                    .iter()
                    .map(|r| r["id"].clone())
                    .collect::<Vec<_>>()
            );
            copy_fields(
                &mut asset,
                &snapshot["parameters"],
                &["provider", "model", "character_id", "parent_id"],
            );
        }
        Ok((
            asset,
            vec![self.root.join(path), self.root.join(thumbnail_path)],
        ))
    }
    fn read_asset(&self, conn: &Connection, args: &Value) -> Result<Value> {
        let asset = get(conn, "asset", required(args, "id")?)?;
        let thumb = args["thumbnail"].as_bool().unwrap_or(false);
        let field = if thumb { "thumbnail_path" } else { "path" };
        let path = checked_path(&self.root, text(&asset, field))?;
        if fs::metadata(&path)?.len() > provider::MAX_IMAGE_BYTES as u64 {
            bail!("Bilddatei überschreitet Größenlimit");
        }
        let bytes = fs::read(path).context("Bilddatei fehlt oder ist nicht lesbar")?;
        if !thumb && digest(&bytes) != text(&asset, "sha256") {
            bail!("Originaldatei wurde außerhalb der App verändert");
        }
        if bytes.len() > provider::MAX_IMAGE_BYTES {
            bail!("Bilddatei überschreitet Größenlimit");
        }
        Ok(
            json!({"id":asset["id"],"mime":if thumb {"image/png"} else {text(&asset,"mime")},"data_base64":STANDARD.encode(bytes)}),
        )
    }
    fn export_payload(&self, conn: &Connection, args: &Value) -> Result<(Value, Vec<Value>)> {
        let mut project = get(conn, "project", required(args, "project_id")?)?;
        let ids = array(args, "asset_ids");
        let assets = if ids.is_empty() {
            list(conn, "asset", Some(text(&project, "id")))?
        } else {
            ids.iter()
                .map(|id| get(conn, "asset", id.as_str().unwrap_or("")))
                .collect::<Result<Vec<_>>>()?
        };
        if assets.iter().any(|a| a["project_id"] != project["id"]) {
            bail!("Asset gehört nicht zum Projekt");
        }
        let jobs = job_history(conn, text(&project, "id"))?;
        let mut style_ids = std::collections::HashSet::new();
        if let Some(id) = project["style_id"].as_str() {
            style_ids.insert(id.to_owned());
        }
        for asset in &assets {
            if let Some(id) = asset["snapshot"]["style"]["id"].as_str() {
                style_ids.insert(id.to_owned());
            }
        }
        for job in &jobs {
            if let Some(id) = job["snapshot"]["style"]["id"].as_str() {
                style_ids.insert(id.to_owned());
            }
        }
        project["backup_styles"] = json!(
            list_styles(conn)?
                .into_iter()
                .filter(|style| style_ids.contains(text(style, "id")))
                .map(|mut style| {
                    if !assets
                        .iter()
                        .any(|asset| asset["id"] == style["preview_asset_id"])
                    {
                        clear_style_preview(&mut style);
                    }
                    style
                })
                .collect::<Vec<_>>()
        );
        project["backup_characters"] = json!(list(conn, "character", Some(text(&project, "id")))?);
        project["backup_jobs"] = json!(jobs);
        Ok((project, assets))
    }
    fn export(&self, conn: &Connection, args: &Value) -> Result<Value> {
        let (project, assets) = self.export_payload(conn, args)?;
        let result = crate::exports::create(&self.root, &project, &assets, args)?;
        self.copy_export(result, text(&settings(conn)?, "export_dir"))
    }
    fn copy_export(&self, result: Value, directory: &str) -> Result<Value> {
        if directory.is_empty() {
            return Ok(result);
        }
        match crate::exports::copy_to_directory(&self.root, &result, Path::new(directory)) {
            Ok(copied) => Ok(copied),
            Err(error) => {
                let mut result = result;
                result["warning"] = json!(format!(
                    "Export intern gesichert; Kopie ins Exportverzeichnis fehlgeschlagen: {error}"
                ));
                Ok(result)
            }
        }
    }
    fn restore(&self, conn: &mut Connection, args: &Value) -> Result<Value> {
        let encoded = text(args, "data_base64");
        if encoded.len() > 86 * 1024 * 1024 {
            bail!("Projektpaket überschreitet 64 MiB");
        }
        let bytes = STANDARD
            .decode(encoded)
            .map_err(|_| anyhow!("Ungültige Base64-Daten"))?;
        let package = crate::exports::restore(&self.root, &bytes)?;
        self.restore_records(conn, package, " · Wiederhergestellt")
    }
    fn restore_records(
        &self,
        conn: &mut Connection,
        mut package: Value,
        name_suffix: &str,
    ) -> Result<Value> {
        validate_package(&package)?;
        normalize_package_previews(&mut package)?;
        let old_project = required(&package["project"], "id")?.to_owned();
        let project_id = new_id();
        let mut ids = HashMap::new();
        ids.insert(old_project, project_id.clone());
        for key in ["assets", "styles", "characters", "jobs"] {
            for item in array(&package, key) {
                if let Some(id) = item["id"].as_str() {
                    ids.insert(id.to_owned(), new_id());
                }
            }
        }
        remap_ids(&mut package, &ids);
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut project = package["project"].clone();
        if let Some(template) = project.get("prompt_template") {
            workspace::validate_template(template)?;
        }
        project["id"] = json!(project_id);
        project["name"] = json!(format!("{}{name_suffix}", text(&project, "name")));
        project["archived"] = json!(false);
        project["deleted_at"] = Value::Null;
        project["created_at"] = json!(now());
        project
            .as_object_mut()
            .unwrap()
            .retain(|k, _| !k.starts_with("backup_"));
        put(&tx, "project", &project)?;
        for (key, kind) in [
            ("styles", "style"),
            ("characters", "character"),
            ("assets", "asset"),
        ] {
            for mut item in array(&package, key) {
                if kind != "style" {
                    item["project_id"] = json!(project_id);
                }
                if kind == "asset" {
                    checked_path(&self.root, required(&item, "path")?)?;
                }
                item["restored_at"] = json!(now());
                put(&tx, kind, &item)?;
            }
        }
        for character in array(&package, "characters") {
            validate_references(&tx, &project_id, &array(&character, "reference_ids"))?;
        }
        // Restored requests are evidence only: never resume paid jobs from an archive.
        for mut job in array(&package, "jobs") {
            job["project_id"] = json!(project_id);
            job["status"] = json!("restored");
            job["idempotency_key"] = json!(format!("restored-{}", text(&job, "id")));
            if job["actual_microusd"].is_number() {
                job["reserved_microusd"] = json!(0);
            } else {
                job["reserved_microusd"] = json!(
                    job["reserved_microusd"]
                        .as_i64()
                        .unwrap_or(0)
                        .max(job["estimated_microusd"].as_i64().unwrap_or(0))
                );
                job["cost_status"] = json!("unknown");
            }
            insert_job(&tx, &job)?;
        }
        tx.commit()?;
        Ok(project)
    }
    fn duplicate_project(&self, conn: &mut Connection, args: &Value) -> Result<Value> {
        let (original, assets) =
            self.export_payload(conn, &json!({"project_id":required(args,"id")?}))?;
        let package = json!({"styles":original["backup_styles"],"characters":original["backup_characters"],"assets":assets,"jobs":[],"project":original});
        // Persist the final name in the same transaction as the copied records.
        // Assets are immutable; duplicate records safely share originals.
        self.restore_records(conn, package, " · Kopie")
    }
    fn submit_export(&self, conn: &mut Connection, args: &Value) -> Result<Value> {
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        get(&tx, "project", required(args, "project_id")?)?;
        let mut options = args.clone();
        if options.get("naming_template").is_none() {
            options["naming_template"] = settings(&tx)?["naming_template"].clone();
        }
        options.as_object_mut().unwrap().remove("idempotency_key");
        let key = args["idempotency_key"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(new_id);
        let previous = list(&tx, "export_job", Some(required(args, "project_id")?))?;
        if let Some(job) = previous.into_iter().find(|j| j["idempotency_key"] == key) {
            if job["options"] != options {
                bail!("Export-Idempotenzschlüssel bereits anderweitig verwendet");
            }
            return Ok(job);
        }
        let (project, assets) = self.export_payload(&tx, &options)?;
        let export_dir = settings(&tx)?["export_dir"].clone();
        let job = json!({"id":new_id(),"project_id":args["project_id"],"created_at":now(),"status":"queued","kind":"export","options":options,"idempotency_key":key,"project_snapshot":project,"assets_snapshot":assets,"export_dir":export_dir});
        put(&tx, "export_job", &job)?;
        tx.commit()?;
        Ok(job)
    }
    fn work_export(&self) -> Result<bool> {
        let conn = self.connect()?;
        let jobs = list(&conn, "export_job", None)?;
        for mut job in jobs.iter().filter(|j| j["status"] == "running").cloned() {
            job["status"] = json!("failed");
            job["error"] = json!(
                "Export durch Neustart unterbrochen; Originale sind unverändert. Export kann erneut beauftragt werden."
            );
            put(&conn, "export_job", &job)?;
        }
        let Some(mut job) = jobs.into_iter().rev().find(|j| j["status"] == "queued") else {
            return Ok(false);
        };
        job["status"] = json!("running");
        put(&conn, "export_job", &job)?;
        let assets = array(&job, "assets_snapshot");
        let result = crate::exports::create(
            &self.root,
            &job["project_snapshot"],
            &assets,
            &job["options"],
        )
        .and_then(|result| self.copy_export(result, text(&job, "export_dir")));
        match result {
            Ok(result) => {
                job["status"] = json!("completed");
                job["result"] = result;
            }
            Err(error) => {
                job["status"] = json!("failed");
                job["error"] = json!(error.to_string());
            }
        }
        job["finished_at"] = json!(now());
        put(&conn, "export_job", &job)?;
        Ok(true)
    }
    pub async fn work_once(&self) -> Result<bool> {
        let lock_path = self.root.join("worker.lock");
        if lock_path.exists() && fs::symlink_metadata(&lock_path)?.file_type().is_symlink() {
            bail!("Worker-Sperre ist ein symbolischer Link");
        }
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)?;
        if lock.try_lock_exclusive().is_err() {
            return Ok(false);
        }
        let job = {
            let mut conn = self.connect()?;
            claim_job(&mut conn)?
        };
        let Some(job) = job else {
            return self.work_export();
        };
        let snapshot = &job["snapshot"];
        if job["kind"] == "prompt_assist" {
            let outcome = assistant::generate(snapshot, job["session_key_required"] == true).await;
            self.finish_assistant(&job, outcome)?;
            FileExt::unlock(&lock)?;
            return Ok(true);
        }
        let references = self.reference_data(snapshot);
        let outcome = match references {
            Ok(references) => {
                provider::generate(snapshot, &references, job["session_key_required"] == true).await
            }
            Err(e) => provider::Outcome {
                images: vec![],
                usage: json!({"not_sent":true}),
                cost: Some(0),
                error: Some(e.to_string()),
                uncertain: false,
                provider_id: None,
            },
        };
        self.finish_job(&job, outcome)?;
        FileExt::unlock(&lock)?;
        Ok(true)
    }
    async fn refresh_prices(&self, args: &Value) -> Result<Value> {
        if args
            .as_object()
            .unwrap()
            .keys()
            .any(|key| key != "provider")
            || args.get("provider").is_some_and(|v| !v.is_string())
        {
            bail!("Preisaktualisierung akzeptiert nur einen Anbieternamen");
        }
        let lock_path = self.root.join("pricing.lock");
        if lock_path.exists() && fs::symlink_metadata(&lock_path)?.file_type().is_symlink() {
            bail!("Preissperre ist ein symbolischer Link");
        }
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)?;
        lock.try_lock_exclusive()
            .context("Eine Preisaktualisierung läuft bereits")?;
        let previous = price_catalog(&self.connect()?)?;
        let catalog = pricing::refresh(&previous, args["provider"].as_str()).await?;
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        put(
            &tx,
            "pricing",
            &json!({"id":"global","created_at":now(),"catalog":catalog}),
        )?;
        tx.commit()?;
        Ok(catalog)
    }
    fn finish_assistant(&self, job: &Value, mut outcome: assistant::Outcome) -> Result<()> {
        let mut conn = self.connect()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut current = get_job(&tx, text(job, "id"))?;
        if job["snapshot"]["parameters"]["target"] == "all" && outcome.error.is_none() {
            let validated = outcome
                .text
                .as_deref()
                .and_then(|s| serde_json::from_str::<Value>(s).ok())
                .filter(|v| {
                    workspace::validate_template(v).is_ok()
                        && ["scene", "character", "instruction"]
                            .iter()
                            .all(|k| v[*k].is_string())
                        && !text(v, "instruction").trim().is_empty()
                });
            if let Some(fields) = validated {
                current["result_sections"] = fields;
            } else {
                outcome.error = Some("KI-Antwort enthält keine drei gültigen Promptbereiche. Nicht automatisch übernommen.".into());
                outcome.text = None;
            }
        }
        current["status"] = json!(if outcome.uncertain {
            "unknown"
        } else if outcome.error.is_some() {
            "failed"
        } else {
            "completed"
        });
        current["result_text"] = json!(outcome.text);
        current["error"] = json!(outcome.error);
        current["actual_microusd"] = json!(outcome.cost);
        current["cost_status"] = json!(if outcome.usage["not_sent"] == true {
            "not_sent"
        } else if outcome.cost.is_some() {
            "usage_calculated"
        } else {
            "unknown"
        });
        current["usage"] = outcome.usage;
        current["provider_request_id"] = json!(outcome.provider_id);
        current["finished_at"] = json!(now());
        if outcome.cost.is_some() {
            current["reserved_microusd"] = json!(0);
        }
        save_job(&tx, &current)?;
        tx.commit()?;
        Ok(())
    }
    fn reference_data(&self, snapshot: &Value) -> Result<Vec<Value>> {
        let mut results = Vec::new();
        let mut total = 0;
        for reference in array(snapshot, "references") {
            let path = checked_path(&self.root, required(&reference, "path")?)?;
            if fs::metadata(&path)?.len() > provider::MAX_IMAGE_BYTES as u64 {
                bail!("Referenzdatei überschreitet Größenlimit");
            }
            let bytes = fs::read(path)?;
            total += bytes.len();
            if total > 20 * 1024 * 1024 {
                bail!("Ausgewählte Referenzen überschreiten insgesamt 20 MiB");
            }
            if digest(&bytes) != text(&reference, "sha256") {
                bail!("Originalreferenz wurde außerhalb der App verändert");
            }
            let (image, format) = provider::validate_image(&bytes)?;
            let (bytes, mime) = if format == image::ImageFormat::Tiff {
                let mut c = Cursor::new(Vec::new());
                image.write_to(&mut c, image::ImageFormat::Png)?;
                (c.into_inner(), "image/png")
            } else {
                (bytes, format.to_mime_type())
            };
            results.push(json!({"mime":mime,"data_base64":STANDARD.encode(bytes)}));
        }
        Ok(results)
    }
    fn finish_job(&self, job: &Value, outcome: provider::Outcome) -> Result<()> {
        let mut conn = self.connect()?;
        // Commit received usage before writing potentially large image files. A later disk failure
        // must not erase known expenses, nor cause this request to be submitted again.
        let mut received = get_job(&conn, text(job, "id"))?;
        received["usage"] = outcome.usage.clone();
        received["actual_microusd"] = json!(outcome.cost);
        received["provider_request_id"] = json!(outcome.provider_id);
        received["outcome_received_at"] = json!(now());
        if outcome.cost.is_some() {
            received["reserved_microusd"] = json!(0);
            received["cost_status"] = json!(if outcome.usage["not_sent"] == true {
                "not_sent"
            } else {
                "usage_calculated"
            });
        } else {
            received["cost_status"] = json!("unknown");
        }
        save_job(&conn, &received)?;
        let mut assets = Vec::new();
        for bytes in &outcome.images {
            match self.store_image(
                bytes,
                text(job, "project_id"),
                text(&job["snapshot"]["parameters"], "description"),
                Some(&job["snapshot"]),
            ) {
                Ok((mut asset, _)) => {
                    asset["job_id"] = job["id"].clone();
                    asset["cost_status"] = json!(if outcome.cost.is_some() {
                        "usage_calculated"
                    } else {
                        "unknown"
                    });
                    assets.push(asset);
                }
                Err(e) => {
                    let mut failed = job.clone();
                    failed["status"] = json!("unknown");
                    failed["usage"] = outcome.usage.clone();
                    failed["actual_microusd"] = json!(outcome.cost);
                    failed["cost_status"] = json!(if outcome.cost.is_some() {
                        "usage_calculated"
                    } else {
                        "unknown"
                    });
                    failed["provider_request_id"] = json!(outcome.provider_id);
                    if outcome.cost.is_some() {
                        failed["reserved_microusd"] = json!(0);
                    }
                    failed["error"] = json!(format!(
                        "Providerantwort erhalten, lokales Schreiben fehlgeschlagen: {e}. Nicht erneut senden."
                    ));
                    save_job(&conn, &failed)?;
                    return Err(e);
                }
            }
        }
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut current = get_job(&tx, text(job, "id"))?;
        let count = assets.len() as i64;
        for (index, mut asset) in assets.into_iter().enumerate() {
            asset["cost_microusd"] = json!(
                outcome
                    .cost
                    .map(|c| c / count + i64::from((index as i64) < c % count))
            );
            put(&tx, "asset", &asset)?;
        }
        let status = if outcome.uncertain {
            "unknown"
        } else if outcome.error.is_some() {
            "failed"
        } else {
            "completed"
        };
        current["status"] = json!(status);
        current["error"] = json!(outcome.error);
        let not_sent = outcome.usage["not_sent"] == true;
        current["usage"] = outcome.usage;
        current["actual_microusd"] = json!(outcome.cost);
        current["cost_status"] = json!(if not_sent {
            "not_sent"
        } else if outcome.cost.is_some() {
            "usage_calculated"
        } else {
            "unknown"
        });
        current["finished_at"] = json!(now());
        current["provider_request_id"] = json!(outcome.provider_id);
        // Unknown expenses retain their reservation; accounting uncertainty must never free budget silently.
        if outcome.cost.is_some() {
            current["reserved_microusd"] = json!(0);
        }
        save_job(&tx, &current)?;
        tx.commit()?;
        Ok(())
    }
}

fn find(conn: &Connection, kind: &str, id: &str) -> Result<Option<Value>> {
    let data: Option<String> = conn
        .query_row(
            "SELECT data FROM records WHERE kind=?1 AND id=?2",
            params![kind, id],
            |r| r.get(0),
        )
        .optional()?;
    data.map(|s| serde_json::from_str(&s).map_err(Into::into))
        .transpose()
}
fn get(conn: &Connection, kind: &str, id: &str) -> Result<Value> {
    find(conn, kind, id)?.ok_or_else(|| anyhow!("{kind} nicht gefunden"))
}
fn put(conn: &Connection, kind: &str, value: &Value) -> Result<()> {
    conn.execute("INSERT INTO records(kind,id,project_id,created_at,data) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(kind,id) DO UPDATE SET project_id=excluded.project_id,data=excluded.data",params![kind,required(value,"id")?,text(value,"project_id"),text(value,"created_at"),serde_json::to_string(value)?])?;
    Ok(())
}
fn list(conn: &Connection, kind: &str, project: Option<&str>) -> Result<Vec<Value>> {
    let mut stmt=conn.prepare("SELECT data FROM records WHERE kind=?1 AND (?2 IS NULL OR project_id=?2) ORDER BY created_at DESC,id")?;
    let rows = stmt.query_map(params![kind, project], |r| r.get::<_, String>(0))?;
    rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
}
fn base_record(args: &Value) -> Value {
    json!({"id":args["id"].as_str().unwrap_or(&new_id()),"created_at":now()})
}
fn settings(conn: &Connection) -> Result<Value> {
    get(conn, "settings", "global")
}
fn default_settings() -> Value {
    json!({"id":"global","created_at":now(),"theme":"system","mode":"simple","language":"de","export_dir":"","naming_template":"{projekt}_{beschreibung}_{nummer:04}","job_limit_microusd":1000000,"project_limit_microusd":10000000,"warning_microusd":500000,"concurrency":1,"automation_approved":false,"rest_enabled":false,"mcp_enabled":false})
}
fn save_settings(conn: &Connection, args: &Value, automated: bool) -> Result<Value> {
    if automated {
        bail!("Zugriffs- und Ausgabenfreigaben können nur lokal in der Anwendung geändert werden");
    }
    let mut value = settings(conn)?;
    copy_fields(
        &mut value,
        args,
        &[
            "theme",
            "language",
            "mode",
            "export_dir",
            "naming_template",
            "job_limit_microusd",
            "project_limit_microusd",
            "warning_microusd",
            "concurrency",
            "automation_approved",
            "rest_enabled",
            "mcp_enabled",
        ],
    );
    if !["light", "dark", "system"].contains(&text(&value, "theme"))
        || !["simple", "expert"].contains(&text(&value, "mode"))
    {
        bail!("Ungültige Anzeigeeinstellung");
    }
    if value.get("language").is_some() && !["de", "en"].contains(&text(&value, "language")) {
        bail!("Unsupported language");
    }
    if value["concurrency"] != 1 {
        bail!("Diese Version führt genau einen Providerauftrag gleichzeitig aus");
    }
    for field in [
        "job_limit_microusd",
        "project_limit_microusd",
        "warning_microusd",
    ] {
        if value[field]
            .as_i64()
            .is_none_or(|n| !(0..=1_000_000_000_000).contains(&n))
        {
            bail!("Ungültige Budgetgrenze");
        }
    }
    for field in ["automation_approved", "rest_enabled", "mcp_enabled"] {
        if !value[field].is_boolean() {
            bail!("Ungültige Freigabeeinstellung");
        }
    }
    if !text(&value, "export_dir").is_empty() {
        let path = Path::new(text(&value, "export_dir"));
        if !path.is_absolute()
            || !path.is_dir()
            || fs::symlink_metadata(path)?.file_type().is_symlink()
        {
            bail!(
                "Exportverzeichnis muss ein existierendes absolutes Verzeichnis ohne symbolischen Link sein"
            );
        }
        let canonical = path.canonicalize()?;
        value["export_dir"] = json!(canonical);
    }
    required(&value, "naming_template")?;
    put(conn, "settings", &value)?;
    Ok(value)
}
fn save_project(conn: &Connection, args: &Value) -> Result<Value> {
    required(args, "name")?;
    let mut value = match args["id"].as_str() {
        Some(id) => get(conn, "project", id)?,
        None => base_record(args),
    };
    copy_fields(
        &mut value,
        args,
        &[
            "name",
            "description",
            "style_id",
            "aspect_ratio",
            "preferences",
            "archived",
            "budget_microusd",
            "prompt_template",
        ],
    );
    if let Some(template) = value.get("prompt_template") {
        workspace::validate_template(template)?;
    }
    if value["description"].is_null() {
        value["description"] = json!("");
    }
    if value["archived"].is_null() {
        value["archived"] = json!(false);
    }
    if value["aspect_ratio"].is_null() {
        value["aspect_ratio"] = json!("1:1");
    }
    if let Some(id) = value["style_id"].as_str().filter(|s| !s.is_empty()) {
        get(conn, "style", id)?;
    }
    if value
        .get("budget_microusd")
        .is_some_and(|v| !v.is_null() && v.as_i64().is_none_or(|n| n < 0))
    {
        bail!("Ungültiges Projektbudget");
    }
    value["updated_at"] = json!(now());
    put(conn, "project", &value)?;
    Ok(value)
}
const DEFAULT_STYLE_PREVIEW_PROMPT: &str = "Eine erwachsene Person in einem hellen Arbeitshemd hält eine weiße Kaffeetasse. Neben ihr steht ein kleiner Retro-Roboter an einer Holzwerkbank. Auf der Werkbank stehen eine bewegliche Metalllampe und eine Topfpflanze. Durch ein Fenster sind entfernte Hügel zu sehen. Zeige Person, Roboter, Holz, Metall, Keramik und Blätter deutlich erkennbar. Alle Motive sollen vollständig im Bild bleiben. Keine bekannten Figuren, Beschriftungen oder Logos.";

fn with_preview_prompt(mut style: Value) -> Value {
    if style.get("preview_prompt").is_none() {
        style["preview_prompt"] = json!(DEFAULT_STYLE_PREVIEW_PROMPT);
    }
    style
}
fn list_styles(conn: &Connection) -> Result<Vec<Value>> {
    let mut styles = list(conn, "style", None)?;
    for style in &mut styles {
        if style["preview_asset_id"].is_null() && style["preview_hidden"] != true {
            let id: Option<String> = conn.query_row("SELECT id FROM records WHERE kind='asset' AND json_extract(data,'$.snapshot.style.id')=?1 AND json_extract(data,'$.provider')!='import' ORDER BY created_at DESC LIMIT 1",[text(style,"id")],|r|r.get(0)).optional()?;
            if let Some(id) = id {
                style["candidate_provider"] = get(conn, "asset", &id)?["provider"].clone();
                style["candidate_asset_id"] = json!(id);
            }
        }
    }
    Ok(styles.into_iter().map(with_preview_prompt).collect())
}
fn validate_preview_prompt(style: &Value) -> Result<()> {
    if let Some(value) = style.get("preview_prompt") {
        let prompt = value.as_str().context("Vorschaumotiv muss Text sein")?;
        if prompt.trim().is_empty() || prompt.len() > 12000 {
            bail!("Vorschaumotiv muss 1 bis 12000 UTF-8-Bytes enthalten");
        }
    }
    Ok(())
}
fn prepare_style_preview(conn: &mut Connection, args: &Value) -> Result<Value> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let style = with_preview_prompt(get(&tx, "style", required(args, "id")?)?);
    validate_preview_prompt(&style)?;
    let project = match list(&tx, "project", None)?.into_iter().find(|p| {
        p["preferences"]["purpose"] == "style_previews"
            && p["archived"] != true
            && p["deleted_at"].is_null()
    }) {
        Some(project) => project,
        None => save_project(
            &tx,
            &json!({"name":"Stilvorschauen","description":"","aspect_ratio":"1:1","preferences":{"purpose":"style_previews"}}),
        )?,
    };
    tx.commit()?;
    Ok(
        json!({"project":project,"description":style["preview_prompt"],"style":style,"aspect_ratio":"1:1","blocks":{"goal":"Ein vergleichbares Vorschaubild für die Stilbibliothek. Übertrage die vorgegebenen Stilmerkmale auf das Motiv.","composition":"Klare Anordnung mit erkennbarem Vorder-, Mittel- und Hintergrund. Alle im Motiv beschriebenen Hauptobjekte gut sichtbar und voneinander unterscheidbar zeigen. Perspektive nach Stilvorgabe.","output":"Ein einzelnes quadratisches Bild, keine Collage und kein Kontaktbogen. Alle Hauptmotive vollständig zeigen.","avoid":"Keine Schrift, Logos, Wasserzeichen oder zusätzlichen Bildrahmen. Keine unbemerkten Ausschnitte der Hauptmotive."}}),
    )
}
fn clear_style_preview(style: &mut Value) {
    if let Some(record) = style.as_object_mut() {
        for field in [
            "preview_asset_id",
            "preview_style_version",
            "preview_provider",
        ] {
            record.remove(field);
        }
    }
}
fn apply_style_preview(style: &mut Value, asset: &Value) -> Result<()> {
    if asset["snapshot"]["style"]["id"] != style["id"] {
        bail!("Das Bild wurde nicht mit diesem Stil erzeugt");
    }
    let version = asset["snapshot"]["style"]["version"]
        .as_u64()
        .context("Stilversion des Vorschaubildes fehlt")?;
    if version == 0 || version > style["version"].as_u64().unwrap_or(0) {
        bail!("Ungültige Stilversion des Vorschaubildes");
    }
    let provider = required(asset, "provider")?;
    if !["test", "gemini", "openai"].contains(&provider) {
        bail!("Unbekannte Herkunft des Vorschaubildes");
    }
    style["preview_hidden"] = json!(false);
    style["preview_asset_id"] = asset["id"].clone();
    style["preview_style_version"] = json!(version);
    style["preview_provider"] = json!(provider);
    Ok(())
}
fn normalize_package_previews(package: &mut Value) -> Result<()> {
    let assets = array(package, "assets");
    let mut styles = array(package, "styles");
    for style in &mut styles {
        match style.get("preview_asset_id") {
            Some(Value::String(id)) => {
                if let Some(asset) = assets.iter().find(|asset| asset["id"] == *id) {
                    apply_style_preview(style, asset)?;
                } else {
                    clear_style_preview(style);
                }
            }
            None | Some(Value::Null) => clear_style_preview(style),
            _ => bail!("Ungültige Vorschaubild-ID im Projektpaket"),
        }
    }
    package["styles"] = json!(styles);
    Ok(())
}
fn save_style(conn: &mut Connection, args: &Value) -> Result<Value> {
    required(args, "name")?;
    validate_preview_prompt(args)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let mut value = match args["id"].as_str() {
        Some(id) => get(&tx, "style", id)?,
        None => base_record(args),
    };
    let old = with_preview_prompt(value.clone());
    let version = value["version"].as_u64().unwrap_or(0) + 1;
    copy_fields(&mut value, args, &["name", "blocks", "preview_prompt"]);
    if !value["blocks"].is_object() {
        bail!("Stilbausteine müssen ein Objekt sein");
    }
    validate_named_blocks(
        &value["blocks"],
        &["medium", "lines", "palette", "light", "texture"],
    )?;
    let mut versions = array(&value, "versions");
    if old["version"].is_number() {
        versions.push(json!({"version":old["version"],"name":old["name"],"blocks":old["blocks"],"preview_prompt":old["preview_prompt"]}));
    }
    value["versions"] = json!(versions);
    value["version"] = json!(version);
    value = with_preview_prompt(value);
    put(&tx, "style", &value)?;
    tx.commit()?;
    Ok(value)
}
fn save_character(conn: &Connection, args: &Value) -> Result<Value> {
    required(args, "name")?;
    let project_id = required(args, "project_id")?;
    get(conn, "project", project_id)?;
    let mut value = match args["id"].as_str() {
        Some(id) => get(conn, "character", id)?,
        None => base_record(args),
    };
    if value["project_id"]
        .as_str()
        .is_some_and(|id| id != project_id)
    {
        bail!("Charakter gehört zu einem anderen Projekt");
    }
    copy_fields(
        &mut value,
        args,
        &[
            "project_id",
            "name",
            "description",
            "features",
            "clothing",
            "colors",
            "binding",
            "reference_ids",
            "approved",
            "portrait_asset_id",
        ],
    );
    for field in ["reference_ids", "binding"] {
        if value.get(field).is_some_and(|v| !v.is_array()) {
            bail!("{field} muss eine Liste sein");
        }
    }
    validate_references(conn, project_id, &array(&value, "reference_ids"))?;
    if let Some(id) = value["portrait_asset_id"]
        .as_str()
        .filter(|id| !id.is_empty())
    {
        let asset = get(conn, "asset", id)?;
        ensure!(
            asset["project_id"] == project_id,
            "Charakterbild gehört zu einem anderen Projekt"
        );
        ensure!(
            asset["provider"] != "import",
            "Als Charakterbild bitte ein erzeugtes Bild auswählen"
        );
    }
    if value["approved"] == true && array(&value, "reference_ids").is_empty() {
        bail!("Bitte zuerst ein Referenzbild auswählen");
    }
    put(conn, "character", &value)?;
    Ok(value)
}
fn update_asset(conn: &Connection, args: &Value) -> Result<Value> {
    let mut asset = get(conn, "asset", required(args, "id")?)?;
    copy_fields(
        &mut asset,
        args,
        &["description", "tags", "favorite", "comment", "status"],
    );
    if text(&asset, "description").len() > 32000 || text(&asset, "comment").len() > 32000 {
        bail!("Text zu lang");
    }
    put(conn, "asset", &asset)?;
    Ok(asset)
}
fn list_assets(conn: &Connection, args: &Value) -> Result<Value> {
    let project = required(args, "project_id")?;
    get(conn, "project", project)?;
    let series_jobs = args
        .get("series_id")
        .map(|id| -> Result<Vec<String>> {
            let series = get(
                conn,
                "series",
                id.as_str().context("Serien-ID muss Text sein")?,
            )?;
            ensure!(
                series["project_id"] == project,
                "Serie gehört nicht zum Projekt"
            );
            Ok(array(&series, "job_ids")
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect())
        })
        .transpose()?;
    let style_id = args
        .get("style_id")
        .map(|value| value.as_str().context("Stil-ID muss Text sein"))
        .transpose()?;
    if let Some(id) = style_id {
        get(conn, "style", id)?;
    }
    let query = text(args, "query").to_lowercase();
    let date = text(args, "date");
    let status = text(args, "status");
    let character = text(args, "character_id");
    let mut all = list(conn, "asset", Some(project))?
        .into_iter()
        .filter(|a| {
            series_jobs
                .as_ref()
                .is_none_or(|ids| ids.iter().any(|id| a["job_id"] == *id))
        })
        .filter(|a| style_id.is_none_or(|id| a["snapshot"]["style"]["id"] == id))
        .filter(|a| {
            (query.is_empty()
                || format!(
                    "{} {} {}",
                    text(a, "description"),
                    a["tags"],
                    text(a, "comment")
                )
                .to_lowercase()
                .contains(&query))
                && (date.is_empty() || text(a, "created_at").starts_with(date))
                && (status.is_empty() || text(a, "status") == status)
                && (character.is_empty() || text(a, "character_id") == character)
                && (!args["favorite"].as_bool().unwrap_or(false) || a["favorite"] == true)
        })
        .collect::<Vec<_>>();
    if let Some(ids) = series_jobs {
        all.sort_by_key(|a| {
            (
                ids.iter()
                    .position(|id| a["job_id"] == *id)
                    .unwrap_or(usize::MAX),
                text(a, "created_at").to_owned(),
                text(a, "id").to_owned(),
            )
        });
    }
    let total = all.len();
    let offset = args["offset"].as_u64().unwrap_or(0) as usize;
    let limit = args["limit"].as_u64().unwrap_or(100).min(200) as usize;
    let items = all
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|mut a| {
            if let Some(version) = a["snapshot"]["style"]["version"].as_u64() {
                a["style_version"] = json!(version);
            }
            a.as_object_mut().unwrap().remove("snapshot");
            a
        })
        .collect::<Vec<_>>();
    Ok(json!({"items":items,"total":total}))
}
fn validate_blocks(blocks: &Value) -> Result<()> {
    if let Some(object) = blocks.as_object() {
        for value in object.values() {
            if value.as_str().is_none_or(|s| s.len() > 8000) {
                bail!("Promptbausteine müssen Texte bis 8000 Zeichen sein");
            }
        }
    } else {
        bail!("Promptbausteine müssen ein Objekt sein");
    }
    Ok(())
}
fn validate_named_blocks(blocks: &Value, allowed: &[&str]) -> Result<()> {
    validate_blocks(blocks)?;
    for key in blocks.as_object().unwrap().keys() {
        if !allowed.contains(&key.as_str()) {
            bail!("Unbekannter Promptbaustein: {key}");
        }
    }
    Ok(())
}
fn validate_references(conn: &Connection, project_id: &str, ids: &[Value]) -> Result<Vec<Value>> {
    if ids.len() > 8 {
        bail!("Höchstens acht Referenzbilder pro Auftrag");
    }
    ids.iter().map(|id|{let asset=get(conn,"asset",id.as_str().unwrap_or(""))?;if text(&asset,"project_id")!=project_id{bail!("Referenzbild gehört nicht zum Projekt");}Ok(json!({"id":asset["id"],"path":asset["path"],"sha256":asset["sha256"],"mime":asset["mime"],"description":asset["description"]}))}).collect()
}
fn build_snapshot(conn: &Connection, args: &Value) -> Result<Value> {
    let allowed = [
        "project_id",
        "description",
        "style_id",
        "character_id",
        "reference_ids",
        "parent_id",
        "provider",
        "model",
        "aspect_ratio",
        "resolution",
        "quality",
        "count",
        "blocks",
        "variants",
        "idempotency_key",
        "unknown_price_approved",
        "expected_price_version",
    ];
    if let Some(object) = args.as_object() {
        for key in object.keys() {
            if !allowed.contains(&key.as_str()) {
                bail!("Nicht unterstützter Generierungsparameter: {key}");
            }
        }
    }
    let project = get(conn, "project", required(args, "project_id")?)?;
    ensure!(
        project["deleted_at"].is_null(),
        "Projekt liegt im Papierkorb"
    );
    required(args, "description")?;
    if project["archived"] == true {
        bail!("Archivierte Projekte können nicht generieren");
    }
    for field in ["provider", "model", "aspect_ratio", "resolution", "quality"] {
        if args.get(field).is_some_and(|v| !v.is_string()) {
            bail!("Modellparameter {field} muss Text sein");
        }
    }
    let mut parameters = args.clone();
    parameters["provider"] = json!(args["provider"].as_str().unwrap_or("test"));
    parameters["model"] = json!(args["model"].as_str().unwrap_or("deterministic-v1"));
    parameters["aspect_ratio"] = json!(args["aspect_ratio"].as_str().unwrap_or("1:1"));
    parameters["resolution"] = json!(args["resolution"].as_str().unwrap_or("1K"));
    parameters
        .as_object_mut()
        .unwrap()
        .remove("idempotency_key");
    parameters.as_object_mut().unwrap().remove("variants");
    parameters["count"] = json!(1);
    if parameters["provider"] == "openai" && parameters.get("quality").is_none() {
        parameters["quality"] = json!("medium");
    }
    provider::validate(&parameters)?;
    let blocks = args.get("blocks").cloned().unwrap_or(json!({}));
    validate_named_blocks(
        &blocks,
        &[
            "goal",
            "scene",
            "character",
            "view",
            "pose",
            "expression",
            "gaze",
            "mouth",
            "composition",
            "output",
            "avoid",
        ],
    )?;
    let style_id = args["style_id"]
        .as_str()
        .filter(|s| !s.is_empty())
        .or(project["style_id"].as_str().filter(|s| !s.is_empty()));
    let style = style_id
        .map(|id| get(conn, "style", id))
        .transpose()?
        .unwrap_or(Value::Null);
    let character = args["character_id"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(|id| get(conn, "character", id))
        .transpose()?
        .unwrap_or(Value::Null);
    if !character.is_null() && character["project_id"] != project["id"] {
        bail!("Charakter gehört nicht zum Projekt");
    }
    if !array(args, "variants").is_empty() && !character.is_null() && character["approved"] != true
    {
        bail!("Für ein Charakterset muss zuerst ein Referenzbild freigegeben werden");
    }
    if args.get("reference_ids").is_some_and(|v| !v.is_array()) {
        bail!("Referenzen müssen eine Liste sein");
    }
    let mut refs = array(args, "reference_ids");
    if character["approved"] == true {
        for reference in array(&character, "reference_ids") {
            if !refs.contains(&reference) {
                refs.push(reference);
            }
        }
    }
    if let Some(parent) = args["parent_id"].as_str().filter(|s| !s.is_empty()) {
        let asset = get(conn, "asset", parent)?;
        if asset["project_id"] != project["id"] {
            bail!("Vorlage gehört nicht zum Projekt");
        }
    }
    let references = validate_references(conn, text(&project, "id"), &refs)?;
    let prompt = compose_prompt(
        &project,
        &style,
        &character,
        &blocks,
        text(args, "description"),
    );
    if prompt.len() > 32000 {
        bail!("Zusammengesetzter Prompt überschreitet 32000 Zeichen");
    }
    Ok(
        json!({"version":1,"created_at":now(),"project":project,"style":style,"character":character,"blocks":blocks,"references":references,"prompt":prompt,"parameters":parameters,"price_snapshot":pricing::snapshot_for(&price_catalog(conn)?, &parameters)?}),
    )
}
fn compose_prompt(
    project: &Value,
    style: &Value,
    character: &Value,
    blocks: &Value,
    description: &str,
) -> String {
    fn fields(value: &Value, labels: &[(&str, &str)]) -> Vec<String> {
        labels
            .iter()
            .filter_map(|(key, label)| {
                let v = text(value, key);
                (!v.trim().is_empty()).then(|| format!("{label}: {v}"))
            })
            .collect()
    }
    let mut sections = Vec::new();
    let mut scene = fields(project, &[("description", "Projektziel")]);
    scene.extend(fields(
        blocks,
        &[
            ("goal", "Verwendung"),
            ("scene", "Szene"),
            ("composition", "Komposition"),
        ],
    ));
    scene.extend(fields(
        &style["blocks"],
        &[
            ("medium", "Medium"),
            ("lines", "Linienführung"),
            ("palette", "Palette"),
            ("light", "Licht"),
            ("texture", "Textur"),
        ],
    ));
    if !scene.is_empty() {
        sections.push(format!("[Gesamte Szene]\n{}", scene.join("\n")));
    }
    let mut figure = fields(
        character,
        &[
            ("name", "Charakter"),
            ("description", "Beschreibung"),
            ("features", "Merkmale"),
            ("clothing", "Kleidung"),
            ("colors", "Farben"),
        ],
    );
    if !array(character, "binding").is_empty() {
        figure.push(format!(
            "Verbindliche Eigenschaften: {}",
            character["binding"]
        ));
    }
    figure.extend(fields(
        blocks,
        &[("character", "Zusätzliche Charakterbeschreibung")],
    ));
    if !figure.is_empty() {
        sections.push(format!("[Charakter]\n{}", figure.join("\n")));
    }
    let mut instruction = vec![format!("Aufgabe: {description}")];
    instruction.extend(fields(
        blocks,
        &[
            ("view", "Ansicht"),
            ("pose", "Pose"),
            ("expression", "Ausdruck"),
            ("gaze", "Blick"),
            ("mouth", "Mundstellung"),
            ("output", "Ausgabe"),
            ("avoid", "Zu vermeiden"),
        ],
    ));
    sections.push(format!("[Einzelanweisung]\n{}", instruction.join("\n")));
    sections.join("\n\n")
}

fn job_count(args: &Value) -> Result<usize> {
    if args.get("variants").is_some_and(|v| !v.is_array()) {
        bail!("Varianten müssen eine konkrete Liste sein");
    }
    let variants = array(args, "variants");
    if !variants.is_empty() {
        if variants.len() > 32 {
            bail!("Höchstens 32 einzeln ausgewählte Varianten");
        }
        return Ok(variants.len());
    }
    let count = match args.get("count") {
        Some(value) => value
            .as_u64()
            .ok_or_else(|| anyhow!("Bildanzahl muss eine ganze positive Zahl sein"))?,
        None => 1,
    };
    if !(1..=8).contains(&count) {
        bail!("Bildanzahl muss zwischen 1 und 8 liegen");
    }
    Ok(count as usize)
}
fn variant_input(args: &Value, variant: Option<&Value>) -> Result<Value> {
    let mut input = args.clone();
    if let Some(variant) = variant {
        validate_blocks(variant)?;
        let mut blocks = input.get("blocks").cloned().unwrap_or(json!({}));
        validate_blocks(&blocks)?;
        for (k, v) in variant.as_object().unwrap() {
            blocks[k] = v.clone();
        }
        input["blocks"] = blocks;
    }
    Ok(input)
}
fn price_catalog(conn: &Connection) -> Result<Value> {
    Ok(find(conn, "pricing", "global")?
        .map(|record| record["catalog"].clone())
        .unwrap_or_else(pricing::bundled_catalog))
}
fn build_assistant_snapshot(conn: &Connection, args: &Value) -> Result<Value> {
    let parameters = assistant::parameters(args)?;
    let project = get(conn, "project", required(args, "project_id")?)?;
    if project["archived"] == true {
        bail!("Archivierte Projekte können keine Assistentenaufträge starten");
    }
    let price = if parameters["provider"] == "test" {
        json!({"version":"test-assistant-v1","currency":"USD","provider":"test","text_input_microusd_per_million":0,"text_output_microusd_per_million":0})
    } else {
        pricing::snapshot_for(&price_catalog(conn)?, &parameters)?
    };
    let mut snapshot = assistant::snapshot(parameters, price);
    snapshot["created_at"] = json!(now());
    // Only a local association; project descriptions/styles are never sent implicitly.
    snapshot["project_id"] = project["id"].clone();
    Ok(snapshot)
}
fn estimate_snapshot(snapshot: &Value, is_assistant: bool) -> Result<Value> {
    if is_assistant {
        assistant::estimate(snapshot)
    } else {
        provider::estimate_at(
            &snapshot["parameters"],
            text(snapshot, "prompt"),
            array(snapshot, "references").len(),
            1,
            &snapshot["price_snapshot"],
        )
    }
}
fn estimate_request(conn: &Connection, args: &Value, is_assistant: bool) -> Result<Value> {
    let snapshot = build_assistant_snapshot(conn, args)?;
    let mut estimate = estimate_snapshot(&snapshot, is_assistant)?;
    let settings = settings(conn)?;
    if estimate["total_microusd"].as_i64().unwrap_or(0)
        >= settings["warning_microusd"].as_i64().unwrap_or(i64::MAX)
    {
        estimate["warnings"]
            .as_array_mut()
            .unwrap()
            .push(json!("Warnschwelle pro Auftrag erreicht."));
    }
    Ok(estimate)
}
fn estimate_batch(conn: &Connection, args: &Value) -> Result<Value> {
    let count = job_count(args)?;
    let variants = array(args, "variants");
    let mut estimates = Vec::new();
    for i in 0..count {
        let input = variant_input(args, variants.get(i))?;
        let snapshot = build_snapshot(conn, &input)?;
        estimates.push(provider::estimate_at(
            &snapshot["parameters"],
            text(&snapshot, "prompt"),
            array(&snapshot, "references").len(),
            1,
            &snapshot["price_snapshot"],
        )?);
    }
    let mut result = estimates[0].clone();
    result["total_microusd"] = json!(
        estimates
            .iter()
            .map(|e| e["total_microusd"].as_i64().unwrap_or(0))
            .sum::<i64>()
    );
    let settings = settings(conn)?;
    let total = result["total_microusd"].as_i64().unwrap_or(0);
    let warnings = result["warnings"].as_array_mut().unwrap();
    if total > 0 && total >= settings["warning_microusd"].as_i64().unwrap_or(0) {
        warnings.push(json!("Warnschwelle pro Auftrag erreicht."));
    }
    let project = get(conn, "project", required(args, "project_id")?)?;
    let budget = project["budget_microusd"]
        .as_i64()
        .unwrap_or(settings["project_limit_microusd"].as_i64().unwrap_or(0));
    let cost = project_cost(conn, required(args, "project_id")?)?;
    let used = cost["calculated_microusd"].as_i64().unwrap_or(0)
        + cost["reserved_microusd"].as_i64().unwrap_or(0);
    if total > 0 && used.saturating_add(total) >= budget * 9 / 10 {
        warnings.push(json!(
            "Mit diesem Auftrag sind mindestens 90 % des Projektbudgets verbraucht oder reserviert."
        ));
    }
    result["items"] = json!(estimates);
    result["count"] = json!(count);
    Ok(result)
}
fn submit(conn: &mut Connection, args: &Value, automated: bool) -> Result<Value> {
    submit_kind(conn, args, automated, false)
}
fn submit_kind(
    conn: &mut Connection,
    args: &Value,
    automated: bool,
    is_assistant: bool,
) -> Result<Value> {
    let key = required(args, "idempotency_key")?;
    if key.len() > 200 {
        bail!("Idempotenzschlüssel zu lang");
    }
    let count = if is_assistant {
        assistant::parameters(args)?;
        1
    } else {
        job_count(args)?
    };
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let existing: Option<String> = tx
        .query_row(
            "SELECT data FROM jobs WHERE idempotency_key=?1",
            [format!("{key}:0")],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(first) = existing {
        let first: Value = serde_json::from_str(&first)?;
        let expected = first["request_fingerprint"].as_str().unwrap_or("");
        let fingerprint = fingerprint(args)?;
        if expected != fingerprint || (first["kind"] == "prompt_assist") != is_assistant {
            bail!("Idempotenzschlüssel wurde bereits für einen anderen Auftrag verwendet");
        }
        let mut jobs = Vec::new();
        for i in 0..first["batch_count"].as_u64().unwrap_or(1) {
            let data: String = tx.query_row(
                "SELECT data FROM jobs WHERE idempotency_key=?1",
                [format!("{key}:{i}")],
                |r| r.get(0),
            )?;
            jobs.push(serde_json::from_str::<Value>(&data)?);
        }
        return Ok(json!({"jobs":jobs,"reused":true}));
    }
    let settings = settings(&tx)?;
    let mut jobs = Vec::new();
    let mut total = 0i64;
    let variants = array(args, "variants");
    for i in 0..count {
        let input = variant_input(args, variants.get(i))?;
        let snapshot = if is_assistant {
            build_assistant_snapshot(&tx, &input)?
        } else {
            build_snapshot(&tx, &input)?
        };
        if automated
            && snapshot["parameters"]["provider"] != "test"
            && settings["automation_approved"] != true
        {
            bail!("Automatisierte Ausgaben sind nicht lokal freigegeben");
        }
        let estimate = estimate_snapshot(&snapshot, is_assistant)?;
        if args
            .get("expected_price_version")
            .is_some_and(|v| !v.is_string() || *v != estimate["price_version"])
        {
            bail!("Preise wurden aktualisiert; Kostenschätzung erneut prüfen");
        }
        let reserved = estimate["total_microusd"]
            .as_i64()
            .ok_or_else(|| anyhow!("Unbekannter Preis; kein Start ohne belastbare Reservierung"))?;
        total = total
            .checked_add(reserved)
            .ok_or_else(|| anyhow!("Kostenüberlauf"))?;
        let session_key_required = crate::secrets::session_key_status()?["providers"]
            [snapshot["parameters"]["provider"].as_str().unwrap_or("")]
            == true;
        jobs.push(json!({"id":new_id(),"kind":if is_assistant {"prompt_assist"} else {"image_generation"},"session_key_required":session_key_required,"project_id":args["project_id"],"status":"queued","created_at":now(),"idempotency_key":format!("{key}:{i}"),"request_fingerprint":fingerprint(args)?,"batch_count":count,"snapshot":snapshot,"estimated_microusd":reserved,"reserved_microusd":reserved,"actual_microusd":null,"cost_status":"estimated","error":null,"cancel_requested":false}));
    }
    reserve_jobs(&tx, &jobs, total, required(args, "project_id")?)?;
    tx.commit()?;
    Ok(json!({"jobs":jobs,"reused":false}))
}
// Shared atomic budget enforcement for ordinary requests and approved series.
fn reserve_jobs(conn: &Connection, jobs: &[Value], total: i64, project_id: &str) -> Result<()> {
    reserve_jobs_with_limit(conn, jobs, total, project_id, None)
}
fn reserve_jobs_with_limit(
    conn: &Connection,
    jobs: &[Value],
    total: i64,
    project_id: &str,
    one_time_limit: Option<i64>,
) -> Result<()> {
    let settings = settings(conn)?;
    if total
        > one_time_limit.unwrap_or_else(|| settings["job_limit_microusd"].as_i64().unwrap_or(0))
    {
        bail!("Auftragsbudget überschritten");
    }
    let project = get(conn, "project", project_id)?;
    let budget = project["budget_microusd"]
        .as_i64()
        .unwrap_or(settings["project_limit_microusd"].as_i64().unwrap_or(0));
    let consumed: i64 = conn.query_row(
        "SELECT COALESCE(SUM(COALESCE(actual,0)+reserved),0) FROM jobs WHERE project_id=?1",
        [project_id],
        |r| r.get(0),
    )?;
    if consumed.checked_add(total).is_none_or(|n| n > budget) {
        bail!("Projektbudget einschließlich reservierter und unklarer Kosten überschritten");
    }
    for job in jobs {
        insert_job(conn, job)?;
    }
    Ok(())
}

fn fingerprint(args: &Value) -> Result<String> {
    Ok(digest(&serde_json::to_vec(args)?))
}
fn insert_job(conn: &Connection, job: &Value) -> Result<()> {
    for field in ["actual_microusd", "estimated_microusd", "reserved_microusd"] {
        validate_money(&job[field])?;
    }
    conn.execute("INSERT INTO jobs(id,project_id,status,idempotency_key,created_at,reserved,actual,cost_status,data)VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![required(job,"id")?,text(job,"project_id"),text(job,"status"),text(job,"idempotency_key"),text(job,"created_at"),job["reserved_microusd"].as_i64().unwrap_or(0),job["actual_microusd"].as_i64(),text(job,"cost_status"),serde_json::to_string(job)?])?;
    Ok(())
}
fn save_job(conn: &Connection, job: &Value) -> Result<()> {
    conn.execute(
        "UPDATE jobs SET status=?2,reserved=?3,actual=?4,cost_status=?5,data=?6 WHERE id=?1",
        params![
            required(job, "id")?,
            text(job, "status"),
            job["reserved_microusd"].as_i64().unwrap_or(0),
            job["actual_microusd"].as_i64(),
            text(job, "cost_status"),
            serde_json::to_string(job)?
        ],
    )?;
    Ok(())
}
fn get_job(conn: &Connection, id: &str) -> Result<Value> {
    let data: Option<String> = conn
        .query_row("SELECT data FROM jobs WHERE id=?1", [id], |r| r.get(0))
        .optional()?;
    Ok(serde_json::from_str(
        &data.ok_or_else(|| anyhow!("Auftrag nicht gefunden"))?,
    )?)
}
fn list_jobs(conn: &Connection, project: Option<&str>) -> Result<Vec<Value>> {
    let mut stmt=conn.prepare("SELECT data FROM jobs WHERE (?1 IS NULL OR project_id=?1) ORDER BY created_at DESC LIMIT 1000")?;
    let rows = stmt.query_map([project], |r| r.get::<_, String>(0))?;
    rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
}
fn job_history(conn: &Connection, project_id: &str) -> Result<Vec<Value>> {
    let mut statement =
        conn.prepare("SELECT data FROM jobs WHERE project_id=?1 ORDER BY created_at,id")?;
    let rows = statement.query_map([project_id], |r| r.get::<_, String>(0))?;
    rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
}
fn cancel_job(conn: &Connection, args: &Value) -> Result<Value> {
    let mut job = get_job(conn, required(args, "id")?)?;
    match text(&job, "status") {
        "queued" => {
            job["status"] = json!("cancelled");
            job["reserved_microusd"] = json!(0);
            job["actual_microusd"] = json!(0);
            job["cost_status"] = json!("not_sent");
        }
        "running" => {
            job["cancel_requested"] = json!(true);
            job["error"] = json!(
                "Abbruch angefordert. Bereits gesendete Provideranfragen können weiter berechnet werden; eintreffende Ergebnisse werden gesichert."
            );
        }
        _ => {}
    }
    save_job(conn, &job)?;
    Ok(job)
}
fn claim_job(conn: &mut Connection) -> Result<Option<Value>> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let running = {
        let mut statement = tx.prepare("SELECT data FROM jobs WHERE status='running'")?;
        let rows = statement.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str::<Value>(&r?)?))
            .collect::<Result<Vec<_>>>()?
    };
    for mut job in running {
        job["status"] = json!("unknown");
        if !job["actual_microusd"].is_number() {
            job["cost_status"] = json!("unknown");
        }
        job["error"] = json!(
            "Anwendung während Verarbeitung beendet. Providerergebnis unklar; keine automatische Wiederholung."
        );
        save_job(&tx, &job)?;
    }
    let sessions = crate::secrets::session_key_status()?["providers"].clone();
    let data: Option<String> = tx
        .query_row(
            "SELECT data FROM jobs WHERE status='queued' AND (COALESCE(json_extract(data,'$.session_key_required'),0)=0 OR (json_extract(data,'$.snapshot.parameters.provider')='gemini' AND ?1) OR (json_extract(data,'$.snapshot.parameters.provider')='openai' AND ?2) OR (json_extract(data,'$.snapshot.parameters.provider')='anthropic' AND ?3)) ORDER BY created_at,COALESCE(json_extract(data,'$.series_id'),id),COALESCE(json_extract(data,'$.series_index'),0),id LIMIT 1",
            params![sessions["gemini"]==true,sessions["openai"]==true,sessions["anthropic"]==true],
            |r| r.get(0),
        )
        .optional()?;
    let result = if let Some(data) = data {
        let mut job: Value = serde_json::from_str(&data)?;
        job["status"] = json!("running");
        job["started_at"] = json!(now());
        save_job(&tx, &job)?;
        Some(job)
    } else {
        None
    };
    tx.commit()?;
    Ok(result)
}
fn project_cost(conn: &Connection, id: &str) -> Result<Value> {
    let (actual,reserved,estimated,unknown):(i64,i64,i64,i64)=conn.query_row("SELECT COALESCE(SUM(COALESCE(actual,0)),0),COALESCE(SUM(reserved),0),COALESCE(SUM(CAST(json_extract(data,'$.estimated_microusd') AS INTEGER)),0),COALESCE(SUM(CASE WHEN cost_status='unknown' THEN 1 ELSE 0 END),0) FROM jobs WHERE project_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?)))?;
    let mut by_kind = json!({});
    for kind in ["image_generation", "prompt_assist"] {
        let (calculated, held, unknown): (i64,i64,i64) = conn.query_row("SELECT COALESCE(SUM(COALESCE(actual,0)),0),COALESCE(SUM(reserved),0),COALESCE(SUM(CASE WHEN cost_status='unknown' THEN 1 ELSE 0 END),0) FROM jobs WHERE project_id=?1 AND COALESCE(json_extract(data,'$.kind'),'image_generation')=?2",params![id,kind],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        by_kind[kind] = json!({"calculated_microusd":calculated,"reserved_microusd":held,"unknown_count":unknown});
    }
    Ok(
        json!({"calculated_microusd":actual,"estimated_microusd":estimated,"reserved_microusd":reserved,"unknown_count":unknown,"currency":"USD","confirmed_microusd":null,"by_kind":by_kind}),
    )
}
fn export_preview(conn: &Connection, args: &Value) -> Result<Value> {
    let project = get(conn, "project", required(args, "project_id")?)?;
    let format = args["format"].as_str().unwrap_or("png");
    let fixed = match format {
        "html" => Some("galerie.html"),
        "pdf" => Some("grafiken.pdf"),
        "pptx" => Some("grafiken.pptx"),
        "zip" => Some("projekt.zip"),
        "sprite" => Some("sprite.png"),
        _ => None,
    };
    if let Some(filename) = fixed {
        return Ok(json!({"filename":filename}));
    }
    if !["original", "png", "jpeg", "webp", "tiff"].contains(&format) {
        bail!("Unbekanntes Exportformat");
    }
    let asset = if let Some(id) = args["asset_id"].as_str() {
        get(conn, "asset", id)?
    } else {
        list(conn, "asset", Some(text(&project, "id")))?
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("Bitte zuerst ein Bild auswählen"))?
    };
    if asset["project_id"] != project["id"] {
        bail!("Asset gehört nicht zum Projekt");
    }
    let mut options = args.clone();
    if options.get("naming_template").is_none() {
        options["naming_template"] = settings(conn)?["naming_template"].clone();
    }
    let base = crate::exports::filename(&project, &asset, &options, 1)?;
    let extension = if format == "original" {
        text(&asset, "format")
    } else {
        format
    };
    Ok(json!({"filename":format!("{base}.{extension}")}))
}
fn validate_money(value: &Value) -> Result<()> {
    if !value.is_null()
        && value
            .as_i64()
            .is_none_or(|n| !(0..=1_000_000_000_000).contains(&n))
    {
        bail!("Ungültiger Geldbetrag im Projektpaket");
    }
    Ok(())
}
fn validate_package(package: &Value) -> Result<()> {
    required(&package["project"], "name")?;
    validate_money(&package["project"]["budget_microusd"])?;
    for key in ["assets", "styles", "characters", "jobs"] {
        for record in array(package, key) {
            Uuid::parse_str(required(&record, "id")?)
                .context("Ungültige Datensatz-ID im Projektpaket")?;
            for field in [
                "cost_microusd",
                "actual_microusd",
                "estimated_microusd",
                "reserved_microusd",
            ] {
                validate_money(&record[field])?;
            }
            if key == "styles" {
                required(&record, "name")?;
                validate_blocks(&record["blocks"])?;
                validate_preview_prompt(&record)?;
            }
            if key == "characters" {
                required(&record, "name")?;
                if array(&record, "reference_ids").len() > 8 {
                    bail!("Zu viele Charakterreferenzen im Projektpaket");
                }
            }
        }
    }
    Ok(())
}
fn remap_ids(value: &mut Value, ids: &HashMap<String, String>) {
    match value {
        Value::Array(items) => {
            for item in items {
                remap_ids(item, ids);
            }
        }
        Value::Object(object) => {
            for (key, item) in object.iter_mut() {
                if key == "id" || key.ends_with("_id") {
                    if let Some(id) = item.as_str().and_then(|s| ids.get(s)) {
                        *item = json!(id);
                    }
                } else if key.ends_with("_ids") {
                    if let Some(items) = item.as_array_mut() {
                        for id in items {
                            if let Some(new) = id.as_str().and_then(|s| ids.get(s)) {
                                *id = json!(new);
                            }
                        }
                    }
                } else {
                    remap_ids(item, ids);
                }
            }
        }
        _ => {}
    }
}
pub fn checked_path(root: &Path, relative: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        bail!("Unzulässiger Dateipfad");
    }
    let mut current = root.to_path_buf();
    for part in path.components() {
        current.push(part);
        if fs::symlink_metadata(&current)?.file_type().is_symlink() {
            bail!("Symbolische Links sind nicht erlaubt");
        }
    }
    let full = current.canonicalize()?;
    if !full.starts_with(root) {
        bail!("Dateipfad außerhalb des Datenverzeichnisses");
    }
    Ok(full)
}
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("Ungültiger Zielpfad"))?;
    if path.exists() {
        bail!("Datei existiert bereits");
    }
    let temp = parent.join(format!(".{}.tmp", new_id()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::hard_link(&temp, path)?;
        fs::remove_file(&temp)?;
        #[cfg(unix)]
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}
fn private_dir(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
fn default_styles() -> Vec<Value> {
    let specs = [
        (
            "Comic mit klarer Kontur",
            "Digitale Tusche",
            "Saubere schwarze Konturen, gleichmäßige Strichstärke",
            "Begrenzte satte Farben",
            "Flaches Licht mit einfachen Schatten",
            "Glatte Farbflächen",
        ),
        (
            "Ligne claire",
            "Tusche und flache Kolorierung",
            "Präzise gleichmäßige Konturen ohne Schraffur",
            "Helle klar getrennte Farben",
            "Gleichmäßige Beleuchtung",
            "Saubere glatte Flächen",
        ),
        (
            "Graphic Novel",
            "Tuscheillustration",
            "Expressive Konturen und Schraffur",
            "Entsättigte Farben, tiefe Schwarztöne",
            "Kräftige dramatische Schatten",
            "Leichte Papierstruktur",
        ),
        (
            "Manga/Anime",
            "Digitale Zeichnung",
            "Feine dynamische Linien, klare Silhouetten",
            "Harmonische lebendige Farben",
            "Cel-Shading, klar begrenzte Schatten",
            "Glatte Flächen",
        ),
        (
            "Cartoon für Erklärfilme",
            "Vektorähnliche Illustration",
            "Runde vereinfachte Formen, konsistente Kontur",
            "Freundliche begrenzte Palette",
            "Weiches flaches Licht",
            "Keine feinen Texturen",
        ),
        (
            "Aquarell",
            "Wasserfarbe auf Papier",
            "Lockere Konturen, weiche Farbübergänge",
            "Transparente lasierende Farben",
            "Helles natürliches Licht",
            "Sichtbare Papierkörnung und Pigmentränder",
        ),
        (
            "Ölgemälde",
            "Ölfarbe auf Leinwand",
            "Malerische Konturen, sichtbare Pinselstriche",
            "Tiefe warme Pigmentfarben",
            "Modellierendes Seitenlicht",
            "Impasto und Leinwandstruktur",
        ),
        (
            "Fotorealistische Darstellung",
            "Fotografie",
            "Natürliche Kanten ohne gezeichnete Konturen",
            "Naturgetreue Farbtemperatur",
            "Plausible Beleuchtung und Schatten",
            "Realistische Materialoberflächen",
        ),
        (
            "Technische Strichzeichnung",
            "Technische Tuschezeichnung",
            "Präzise dünne Linien und definierte Kanten",
            "Schwarz auf Weiß",
            "Keine dekorativen Lichteffekte",
            "Keine Textur, klare Details",
        ),
        (
            "Isometrische Illustration",
            "Geometrische Illustration",
            "Parallele Achsen, konsistente isometrische Perspektive",
            "Begrenzte abgestimmte Palette",
            "Einheitliche Lichtquelle von links oben",
            "Saubere Flächen",
        ),
        (
            "Pixel Art",
            "Rasterillustration",
            "Scharfe Pixelkanten ohne Kantenglättung",
            "Kleine festgelegte Farbpalette",
            "Abgestufte Pixel-Schatten",
            "Bewusst grobes Pixelraster",
        ),
    ];
    specs.iter().enumerate().map(|(i,(name,medium,lines,palette,light,texture))|json!({"id":format!("00000000-0000-4000-8000-{:012}",i+1),"name":name,"version":1,"versions":[],"created_at":"2026-10-31T00:00:00Z","blocks":{"medium":medium,"lines":lines,"palette":palette,"light":light,"texture":texture}})).collect()
}

#[cfg(test)]
mod imported_money_tests {
    use super::*;
    #[test]
    fn archive_negative_cost_cannot_reduce_budget() {
        let package = json!({"project":{"name":"Synthetic"},"jobs":[{"id":"00000000-0000-4000-8000-000000000001","actual_microusd":-10}]});
        assert!(validate_package(&package).is_err());
    }
    #[test]
    fn id_remapping_preserves_historical_prompt_text() {
        let id = "00000000-0000-4000-8000-000000000001";
        let mut record = json!({"id":id,"prompt":id,"description":id,"reference_ids":[id]});
        let mut map = HashMap::new();
        map.insert(id.into(), "replacement".into());
        remap_ids(&mut record, &map);
        assert_eq!(record["id"], "replacement");
        assert_eq!(record["prompt"], id);
        assert_eq!(record["description"], id);
        assert_eq!(record["reference_ids"][0], "replacement");
    }
}
