//! Local transports: one core, authenticated loopback HTTP and official rmcp stdio.
use crate::{core::Core, secrets, tokens};
use anyhow::{Context, Result, ensure};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
};
use rmcp::{ErrorData, RoleServer, ServerHandler, ServiceExt, model::*, service::RequestContext};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};

// An omitted provider preserves the original Gemini IPC contract. Invalid types never default.
fn key_provider(args: &Value) -> Result<&str> {
    match args.get("provider") {
        None => Ok("gemini"),
        Some(value) => value.as_str().context("Schlüsselanbieter muss Text sein"),
    }
}

pub const OPERATIONS: &[&str] = &[
    "state",
    "project.get",
    "asset.get",
    "project.list",
    "project.save",
    "project.prompt.save",
    "project.duplicate",
    "style.list",
    "style.save",
    "style.duplicate",
    "style.preview.prepare",
    "style.preview.set",
    "character.list",
    "character.save",
    "asset.import",
    "asset.list",
    "asset.update",
    "asset.read",
    "prompt.preview",
    "cost.estimate",
    "pricing.catalog",
    "pricing.refresh",
    "assistant.models",
    "assistant.estimate",
    "assistant.submit",
    "job.submit",
    "series.submit",
    "series.list",
    "series.get",
    "series.quote",
    "series.cancel",
    "job.list",
    "job.get",
    "job.cancel",
    "cost.project",
    "export.create",
    "export.preview",
    "export.submit",
    "export.get",
    "export.read",
    "project.restore",
];
#[derive(Clone)]
pub struct Bridge {
    pub core: Core,
}
impl Bridge {
    pub fn new(core: Core) -> Result<Self> {
        let bridge = Self { core };
        bridge.tokens()?;
        Ok(bridge)
    }
    fn tokens(&self) -> Result<tokens::Store> {
        tokens::Store::open(self.core.root())
    }
    pub fn create_token(&self) -> Result<Value> {
        self.tokens()?.create(&json!({}))
    }
    pub fn authorized(&self, token: &str) -> bool {
        self.authorized_for(token, "rest")
    }
    pub fn authorized_for(&self, token: &str, channel: &str) -> bool {
        self.tokens()
            .is_ok_and(|store| store.authorized(token, channel))
    }
    pub async fn enabled(&self, name: &str) -> bool {
        self.core
            .call("settings.get", json!({}), false)
            .await
            .ok()
            .and_then(|s| s[name].as_bool())
            .unwrap_or(false)
    }
    pub async fn call_authenticated(
        &self,
        op: &str,
        args: Value,
        token: &str,
        channel: &str,
    ) -> Result<Value> {
        let access = self
            .tokens()?
            .authenticated_access(token, channel)?
            .context("Zugriffstoken ungültig oder widerrufen")?;
        if op == "series.submit" {
            return self.core.submit_series(&args,json!({"channel":channel,"access_id":access["id"],"name":access["name"],"recipient":access["recipient"],"purpose":access["purpose"]}));
        }
        self.call(op, args, true).await
    }
    pub async fn call(&self, op: &str, args: Value, automated: bool) -> Result<Value> {
        if automated {
            ensure!(
                OPERATIONS.contains(&op),
                "Diese Operation ist nur in der Desktop-App erlaubt"
            );
        }
        match op {
            "secret.check" => match args.get("source").and_then(Value::as_str) {
                Some("keychain") => secrets::check_stored_provider_key_for(key_provider(&args)?),
                None if args.get("source").is_none() => {
                    secrets::check_provider_key_for(key_provider(&args)?)
                }
                Some("active") => secrets::check_provider_key_for(key_provider(&args)?),
                _ => anyhow::bail!("Unbekannte Schlüsselquelle"),
            },
            "secret.session.set" => secrets::set_session_key_for(
                key_provider(&args)?,
                args["key"].as_str().context("Schlüssel fehlt")?,
            ),
            "secret.session.remove" => secrets::remove_session_key_for(key_provider(&args)?),
            "secret.session.status" => secrets::session_key_status(),
            "secret.set" => {
                secrets::set_provider_key_for(
                    key_provider(&args)?,
                    args["key"].as_str().context("Schlüssel fehlt")?,
                )?;
                Ok(secrets::key_status())
            }
            "secret.delete" => {
                secrets::delete_provider_key_for(key_provider(&args)?)?;
                Ok(secrets::key_status())
            }
            "tokens.create" => self.tokens()?.create(&args),
            "tokens.list" => self.tokens()?.list(),
            "tokens.update" => self.tokens()?.update(&args),
            "tokens.revoke" => self.tokens()?.revoke(&args),
            "asset.reveal" => self.reveal(&args).await,
            "export.reveal" => self.reveal_export(&args),
            "help.provider" => {
                let url = match args["provider"].as_str() {
                    Some("gemini") => "https://aistudio.google.com/api-keys",
                    Some("openai") => "https://platform.openai.com/api-keys",
                    Some("anthropic") => "https://platform.claude.com/settings/keys",
                    _ => anyhow::bail!("Unknown provider"),
                };
                #[cfg(target_os = "macos")]
                let status = std::process::Command::new("open").arg(url).status()?;
                #[cfg(target_os = "windows")]
                let status = std::process::Command::new("rundll32.exe")
                    .args(["url.dll,FileProtocolHandler", url])
                    .status()?;
                #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                let status = std::process::Command::new("xdg-open").arg(url).status()?;
                ensure!(status.success(), "Could not open browser");
                Ok(json!({"opened": true}))
            }
            "storage.configure" => {
                configure_storage(args["path"].as_str().context("Verzeichnis fehlt")?)
            }
            _ => self.core.call(op, args, automated).await,
        }
    }
    fn reveal_export(&self, args: &Value) -> Result<Value> {
        let id = args["id"].as_str().context("Export-ID fehlt")?;
        uuid::Uuid::parse_str(id).context("Ungültige Export-ID")?;
        let root = self.core.root().join("exports").canonicalize()?;
        let path = root.join(id).canonicalize()?;
        ensure!(path.starts_with(root), "Ungültiger Exportpfad");
        show_path(&path)?;
        Ok(json!({"revealed":true}))
    }
    async fn reveal(&self, args: &Value) -> Result<Value> {
        let id = args["id"].as_str().context("Asset-ID fehlt")?;
        // Metadata path is core-owned, never a caller-controlled filesystem path.
        let asset = self.core.call("asset.get", json!({"id":id}), false).await?;
        let root = self.core.root().canonicalize()?;
        let relative = asset["path"].as_str().context("Originalpfad fehlt")?;
        ensure!(
            std::path::Path::new(relative)
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_))),
            "Ungültiger Dateipfad"
        );
        let path = root.join(relative).canonicalize()?;
        ensure!(path.starts_with(&root), "Ungültiger Dateipfad");
        show_path(&path)?;
        Ok(json!({"revealed":true}))
    }
}
fn show_path(path: &std::path::Path) -> Result<()> {
    #[cfg(target_os = "macos")]
    std::process::Command::new("/usr/bin/open")
        .arg("-R")
        .arg(path)
        .spawn()?;
    #[cfg(target_os = "windows")]
    std::process::Command::new("explorer.exe")
        .arg(format!("/select,{}", path.display()))
        .spawn()?;
    Ok(())
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Rpc {
    op: String,
    #[serde(default)]
    args: Value,
}
#[derive(Clone)]
struct HttpState {
    bridge: Bridge,
    port: u16,
}
fn check_headers(headers: &HeaderMap, state: &HttpState) -> Result<()> {
    let host = headers
        .get("host")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    ensure!(
        host == format!("127.0.0.1:{}", state.port),
        "Host nicht zugelassen"
    );
    ensure!(
        !headers.contains_key("origin"),
        "Browser-Origin nicht zugelassen"
    );
    let token = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .unwrap_or("");
    ensure!(
        state.bridge.authorized(token),
        "Zugriffstoken fehlt oder wurde widerrufen"
    );
    Ok(())
}
async fn request(state: HttpState, headers: HeaderMap, rpc: Rpc) -> axum::response::Response {
    if !state.bridge.enabled("rest_enabled").await || check_headers(&headers, &state).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"Lokaler API-Zugriff nicht autorisiert"})),
        )
            .into_response();
    }
    let token = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .unwrap_or("");
    match state
        .bridge
        .call_authenticated(&rpc.op, rpc.args, token, "rest")
        .await
    {
        Ok(value) => (StatusCode::OK, Json(value)).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":e.to_string()})),
        )
            .into_response(),
    }
}
async fn rpc_handler(
    State(s): State<HttpState>,
    h: HeaderMap,
    Json(r): Json<Rpc>,
) -> axum::response::Response {
    request(s, h, r).await
}
async fn operation_handler(
    State(s): State<HttpState>,
    h: HeaderMap,
    Path(op): Path<String>,
    Json(args): Json<Value>,
) -> axum::response::Response {
    request(s, h, Rpc { op, args }).await
}
async fn projects_handler(State(s): State<HttpState>, h: HeaderMap) -> axum::response::Response {
    request(
        s,
        h,
        Rpc {
            op: "project.list".into(),
            args: json!({}),
        },
    )
    .await
}
async fn job_handler(
    State(s): State<HttpState>,
    h: HeaderMap,
    Path(id): Path<String>,
) -> axum::response::Response {
    request(
        s,
        h,
        Rpc {
            op: "job.get".into(),
            args: json!({"id":id}),
        },
    )
    .await
}
async fn schema_handler(State(s): State<HttpState>, h: HeaderMap) -> axum::response::Response {
    if !s.bridge.enabled("rest_enabled").await || check_headers(&h, &s).is_err() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Json(openapi()).into_response()
}
pub fn operation_schema(op: &str) -> Value {
    if op == "series.quote" {
        return json!({"type":"object","properties":{"id":{"type":"string"},"selection":{"type":"object","properties":{"provider":{"type":"string","enum":["gemini","openai","test"]},"model":{"type":"string"},"resolution":{"type":"string"},"quality":{"type":"string"}},"required":["provider","model","resolution"],"additionalProperties":false}},"required":["id"],"additionalProperties":false});
    }
    if op == "series.submit" {
        let mut item = operation_schema("job.submit");
        item["properties"]
            .as_object_mut()
            .unwrap()
            .remove("project_id");
        item["properties"]
            .as_object_mut()
            .unwrap()
            .remove("idempotency_key");
        item["required"] = json!(["description"]);
        return json!({"type":"object","properties":{"project_id":{"type":"string"},"description":{"type":"string","maxLength":1000},"idempotency_key":{"type":"string","maxLength":200},"items":{"type":"array","minItems":1,"maxItems":32,"items":item}},"required":["project_id","description","idempotency_key","items"],"additionalProperties":false});
    }
    let generation = "project_id description style_id character_id reference_ids parent_id provider model aspect_ratio resolution quality count blocks variants idempotency_key unknown_price_approved expected_price_version";
    let fields = match op {
        "project.save" => {
            "id name description style_id aspect_ratio preferences archived budget_microusd prompt_template"
        }
        "project.prompt.save" => "id prompt_template",
        "project.get"
        | "project.duplicate"
        | "style.duplicate"
        | "asset.get"
        | "series.get"
        | "series.cancel"
        | "job.get"
        | "job.cancel"
        | "export.get"
        | "export.read"
        | "style.preview.prepare" => "id",
        "style.save" => "id name blocks preview_prompt",
        "style.preview.set" => "id asset_id",
        "character.save" => {
            "id project_id name description features clothing colors binding reference_ids approved portrait_asset_id"
        }
        "series.list" | "character.list" | "job.list" | "cost.project" => "project_id",
        "asset.import" => "project_id name description data_base64",
        "asset.list" => {
            "project_id query character_id style_id series_id favorite status date offset limit"
        }
        "asset.update" => "id description tags favorite comment status",
        "asset.read" => "id thumbnail",
        "prompt.preview" | "cost.estimate" | "job.submit" => generation,
        "pricing.refresh" => "provider",
        "assistant.estimate" => "project_id text instruction provider model target",
        "assistant.submit" => {
            "project_id text instruction provider model idempotency_key expected_price_version target"
        }
        "export.create" | "export.submit" | "export.preview" => {
            "project_id asset_ids format title description include_metadata width height fit background layout cell_width cell_height columns padding naming_template page_width_pt page_height_pt idempotency_key"
        }
        "project.restore" => "data_base64",
        _ => "",
    };
    let mut properties = serde_json::Map::new();
    for field in fields.split_whitespace() {
        let kind = match field {
            "archived"
            | "approved"
            | "favorite"
            | "thumbnail"
            | "include_metadata"
            | "unknown_price_approved" => "boolean",
            "count" | "offset" | "limit" | "width" | "height" | "cell_width" | "cell_height"
            | "columns" | "padding" | "page_width_pt" | "page_height_pt" | "budget_microusd" => {
                "integer"
            }
            "binding" | "reference_ids" | "asset_ids" | "tags" | "variants" => "array",
            "blocks" | "preferences" | "prompt_template" => "object",
            _ => "string",
        };
        let mut schema = json!({"type":kind});
        if kind == "array" {
            schema["items"] = if field == "variants" {
                json!({"type":"object","properties":{"pose":{"type":"string"},"expression":{"type":"string"},"view":{"type":"string"},"mouth":{"type":"string"}},"additionalProperties":false})
            } else {
                json!({"type":"string"})
            };
        }
        if op == "style.preview.set" && field == "asset_id" {
            schema["type"] = json!(["string", "null"]);
            schema["description"] = json!(
                "Generated asset with matching style snapshot; explicit null clears only the preview link."
            );
        }
        if field == "provider" {
            schema["enum"] = if op.starts_with("assistant.") {
                json!(["anthropic", "test"])
            } else if op == "pricing.refresh" {
                json!(["gemini", "openai", "anthropic"])
            } else {
                json!(["test", "gemini", "openai"])
            };
        }
        if field == "target" {
            schema["enum"] = json!(["scene", "character", "instruction", "all"]);
        }
        if field == "prompt_template" {
            schema = json!({"type":"object","properties":{"scene":{"type":"string"},"character":{"type":"string"},"instruction":{"type":"string"}},"additionalProperties":false});
        }
        if field == "quality" {
            schema["enum"] = json!(["low", "medium", "high", "xhigh", "max"]);
        }
        if field == "format" {
            schema["enum"] = json!([
                "original", "png", "jpeg", "webp", "tiff", "html", "pdf", "pptx", "zip", "sprite"
            ]);
        }
        if field == "fit" {
            schema["enum"] = json!(["preserve", "contain", "cover", "fill"]);
        }
        if field == "layout" {
            schema["enum"] = json!(["single", "contact"]);
        }
        if field == "idempotency_key" {
            schema["description"] = json!(
                "Stable local request identity. Reuse only with identical arguments; changed request needs a new key."
            );
        }
        if field == "data_base64" {
            schema["description"] = json!(
                "Inline file bytes, never a path or URL. Image max20MiB; project ZIP max64MiB."
            );
        }
        properties.insert(field.to_owned(), schema);
    }
    let mut schema = json!({"type":"object","properties":properties,"additionalProperties":false});
    if op == "style.preview.prepare" {
        schema["required"] = json!(["id"]);
    } else if op == "style.preview.set" {
        schema["required"] = json!(["id", "asset_id"]);
    } else if op == "assistant.estimate" {
        schema["required"] = json!(["project_id", "text"]);
    } else if op == "assistant.submit" {
        schema["required"] = json!(["project_id", "text", "idempotency_key"]);
    }
    schema
}
pub fn openapi() -> Value {
    let mut paths = serde_json::Map::new();
    for op in OPERATIONS {
        paths.insert(format!("/v1/operations/{op}"),json!({"post":{"operationId":op.replace('.',"_"),"summary":op,"security":[{"localToken":[]}],"requestBody":{"required":true,"content":{"application/json":{"schema":operation_schema(op)}}},"responses":{"200":{"description":"Core result; generation returns persistent job IDs"},"400":{"description":"Validation or budget error"},"401":{"description":"Disabled or unauthorized"}}}}));
    }
    json!({"openapi":"3.1.0","info":{"title":"FrameStudio local API","version":"0.1.0","description":"Loopback only. Request shapes: docs/CONTRACT.md and API.md. Integer micro-USD, null costs unknown."},"servers":[{"url":"http://127.0.0.1:43120"}],"paths":paths,"components":{"securitySchemes":{"localToken":{"type":"http","scheme":"bearer"}}}})
}
pub async fn serve_http(bridge: Bridge, port: u16) -> Result<()> {
    ensure!(
        bridge.enabled("rest_enabled").await,
        "REST ist deaktiviert; zuerst in Einstellungen aktivieren"
    );
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    let router = Router::new()
        .route("/rpc", post(rpc_handler))
        .route("/v1/operations/{op}", post(operation_handler))
        .route("/v1/projects", get(projects_handler))
        .route("/v1/jobs/{id}", get(job_handler))
        .route("/openapi.json", get(schema_handler))
        .layer(DefaultBodyLimit::max(90 * 1024 * 1024))
        .with_state(HttpState { bridge, port });
    axum::serve(listener, router).await?;
    Ok(())
}
#[derive(Clone)]
struct Mcp {
    bridge: Bridge,
    // In-memory only, supplied by the local host; never emitted through MCP or metadata.
    token: Arc<String>,
}
impl Mcp {
    async fn access_allowed(&self) -> bool {
        self.bridge.enabled("mcp_enabled").await && self.bridge.authorized_for(&self.token, "mcp")
    }
}
impl ServerHandler for Mcp {
    fn get_info(&self) -> ServerConfig {
        let mut info = ServerConfig::default();
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.server_info.name = "framestudio".into();
        info.server_info.version = env!("CARGO_PKG_VERSION").into();
        info.instructions=Some("Local image workshop. For a complete image series use series_submit with description and concrete items. It returns a pending approval with origin, count and total estimate, not running jobs. The user approves the entire frozen series once in FrameStudio; poll series_get for job IDs/status. Never split a series into per-image approval requests. series_approve is local only. Use cost_estimate before direct job_submit. Each operation uses the same project store and spend rules as GUI. Paid automation must be authorized in GUI. asset_read returns actual image content, export_read returns base64 file bytes. No shell or arbitrary paths. See docs/CONTRACT.md for payload fields.".into());
        info
    }
    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> std::result::Result<ListToolsResult, ErrorData> {
        if !self.access_allowed().await {
            return Err(ErrorData::invalid_request(
                "MCP-Zugang deaktiviert oder widerrufen",
                None,
            ));
        }
        let result = ListToolsResult { tools:OPERATIONS.iter().map(|op|Tool::new(op.replace('.',"_"),format!("FrameStudio {op}. Same JSON payload as REST; see docs/CONTRACT.md. All costs integer micro-USD."), Arc::new(operation_schema(op).as_object().unwrap().clone()))).collect(), ..Default::default() };
        Ok(result)
    }
    async fn call_tool(
        &self,
        r: CallToolRequestParams,
        _: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResponse, ErrorData> {
        if !self.access_allowed().await {
            return Ok(CallToolResult::error(vec![ContentBlock::text(
                "MCP-Zugang deaktiviert oder widerrufen.",
            )])
            .into());
        }
        let Some(op) = OPERATIONS.iter().find(|op| op.replace('.', "_") == r.name) else {
            return Err(ErrorData::invalid_params("Unbekanntes Werkzeug", None));
        };
        let args = Value::Object(r.arguments.unwrap_or_default());
        let result = match self
            .bridge
            .call_authenticated(op, args, &self.token, "mcp")
            .await
        {
            Ok(v) => {
                let mut content = vec![ContentBlock::text(v.to_string())];
                if *op == "asset.read"
                    && let (Some(data), Some(mime)) =
                        (v["data_base64"].as_str(), v["mime"].as_str())
                {
                    content = vec![ContentBlock::image(data, mime)];
                }
                let mut result = CallToolResult::success(content);
                result.structured_content = Some(v);
                result
            }
            Err(e) => CallToolResult::error(vec![ContentBlock::text(e.to_string())]),
        };
        Ok(result.into())
    }
}
pub async fn serve_mcp(bridge: Bridge) -> Result<()> {
    ensure!(
        bridge.enabled("mcp_enabled").await,
        "MCP ist deaktiviert; zuerst in Einstellungen aktivieren"
    );
    let token = std::env::var("FRAMESTUDIO_MCP_TOKEN")
        .context("MCP-Zugriffstoken fehlt; FRAMESTUDIO_MCP_TOKEN im lokalen Host setzen")?;
    ensure!(
        bridge.authorized_for(&token, "mcp"),
        "MCP-Zugriffstoken ungültig, widerrufen oder nicht für MCP freigegeben"
    );
    let worker_task = tokio::spawn(worker(bridge.core.clone()));
    let server = Mcp {
        bridge,
        token: Arc::new(token),
    }
    .serve(rmcp::transport::stdio())
    .await;
    let result = match server {
        Ok(server) => server
            .waiting()
            .await
            .map(|_| ())
            .map_err(anyhow::Error::from),
        Err(error) => Err(error.into()),
    };
    worker_task.abort();
    result?;
    Ok(())
}
fn app_dirs() -> Result<directories::ProjectDirs> {
    directories::ProjectDirs::from("com", "YorkStack", "FrameStudio")
        .context("Benutzerverzeichnis nicht verfügbar")
}
pub fn default_root() -> Result<PathBuf> {
    let dirs = app_dirs()?;
    let config = dirs.config_dir().join("storage.json");
    if config.exists() {
        ensure!(
            !std::fs::symlink_metadata(&config)?.file_type().is_symlink(),
            "Unsichere Speicherpfadkonfiguration"
        );
        let data: Value = serde_json::from_slice(&std::fs::read(config)?)?;
        return Ok(PathBuf::from(
            data["path"]
                .as_str()
                .context("Ungültige Speicherpfadkonfiguration")?,
        ));
    }
    Ok(dirs.data_local_dir().to_path_buf())
}
fn validate_workspace(path: &str) -> Result<PathBuf> {
    use std::io::Write;
    ensure!(
        !path.is_empty() && path.len() < 2000,
        "Ungültiger Speicherpfad"
    );
    let path = PathBuf::from(path);
    ensure!(
        path.is_absolute()
            && !path
                .components()
                .any(|p| matches!(p, std::path::Component::ParentDir)),
        "Absoluter Speicherpfad erforderlich"
    );
    for ancestor in path.ancestors() {
        ensure!(
            !std::fs::symlink_metadata(ancestor).is_ok_and(|m| m.file_type().is_symlink()),
            "Symbolische Links im Speicherpfad sind nicht erlaubt"
        );
    }
    std::fs::create_dir_all(&path)?;
    let path = path.canonicalize()?;
    // Refuse arbitrary occupied directories; existing FrameStudio workspaces are welcome.
    let occupied =
        std::fs::read_dir(&path)?.any(|entry| entry.is_ok_and(|e| e.file_name() != ".DS_Store"));
    ensure!(
        !occupied || path.join("framestudio.sqlite3").is_file(),
        "Choose an empty folder or an existing FrameStudio workspace"
    );
    let probe = path.join(format!(".framestudio-write-{}", uuid::Uuid::new_v4()));
    {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&probe)?;
        file.write_all(b"FrameStudio")?;
        file.sync_all()?;
    }
    std::fs::remove_file(probe)?;
    Ok(path)
}
fn configure_storage(path: &str) -> Result<Value> {
    use std::io::Write;
    let path = validate_workspace(path)?;
    let dirs = app_dirs()?;
    std::fs::create_dir_all(dirs.config_dir())?;
    let temp = dirs
        .config_dir()
        .join(format!(".storage-{}.part", uuid::Uuid::new_v4()));
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp)?;
    file.write_all(&serde_json::to_vec(&json!({"path":path}))?)?;
    file.sync_all()?;
    std::fs::rename(temp, dirs.config_dir().join("storage.json"))?;
    Ok(
        json!({"path":path,"restart_required":true,"notice":"Beim nächsten Start ohne --data-dir wird dieser Datenbestand geöffnet. Bisherige Daten bleiben am bisherigen Ort."}),
    )
}
pub async fn worker(core: Core) {
    loop {
        if let Err(_error) = core.work_once().await { /* Durable job errors are exposed via job status, never raw provider bodies in logs. */
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
}
pub async fn monitor_http(bridge: Bridge) {
    let mut task: Option<tokio::task::JoinHandle<Result<()>>> = None;
    loop {
        let active = bridge.enabled("rest_enabled").await;
        if active && task.as_ref().is_none_or(|t| t.is_finished()) {
            task = Some(tokio::spawn(serve_http(bridge.clone(), 43120)));
        } else if !active && let Some(t) = task.take() {
            t.abort();
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}

#[cfg(test)]
mod workspace_selection_tests {
    use super::*;
    #[test]
    fn validates_workspace_without_modifying_previous_data_or_global_configuration() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let path = root.join("new-workspace");
        assert_eq!(
            validate_workspace(path.to_str().unwrap()).unwrap(),
            path.canonicalize().unwrap()
        );
        let unrelated = root.join("documents");
        std::fs::create_dir(&unrelated).unwrap();
        std::fs::write(unrelated.join("keep.txt"), b"preserve me").unwrap();
        assert!(validate_workspace(unrelated.to_str().unwrap()).is_err());
        assert_eq!(
            std::fs::read(unrelated.join("keep.txt")).unwrap(),
            b"preserve me"
        );
        assert!(validate_workspace("relative/path").is_err());
        assert!(validate_workspace(&format!("{}/../escape", root.display())).is_err());
        let existing = root.join("existing");
        let core = Core::open(existing.clone()).unwrap();
        assert_eq!(
            validate_workspace(existing.to_str().unwrap()).unwrap(),
            existing.canonicalize().unwrap()
        );
        drop(core);
        #[cfg(unix)]
        {
            let link = root.join("alias");
            std::os::unix::fs::symlink(&existing, &link).unwrap();
            assert!(validate_workspace(link.to_str().unwrap()).is_err());
        }
    }
}
