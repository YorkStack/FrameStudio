# FrameStudio operation contract (v1)

*Deutsch: [de/CONTRACT.md](de/CONTRACT.md)*

One Rust crate `framestudio` under `src-tauri`, with the optional feature `desktop`. The same operations serve GUI, REST and MCP; see [API.md](API.md) for transport details and [openapi.json](openapi.json) for the machine-readable schema.

## Core Rust boundary

`pub struct Core` is Clone + Send + Sync and holds the root `PathBuf`. `pub fn open(root: PathBuf) -> anyhow::Result<Self>` migrates the database. `pub async fn call(&self, op: &str, args: serde_json::Value, automated: bool) -> anyhow::Result<Value>` dispatches all methods. `pub async fn work_once(&self) -> Result<bool>` atomically claims one job; a process lock prevents multiple workers. `main` runs the worker loop. `pub fn root(&self) -> &Path`. The core checks automated spending approval, durable budgets and idempotency.

All JSON is snake_case. Objects use a string UUID `id`. Money is an integer `*_microusd`; `null` means unknown. Calls return records directly or arrays (list calls). Error messages are human-readable and currently in German.

## Operations and payloads

- `state {}` → `{projects,styles,characters,settings,models,assistant_models,pricing_revision,key_status,data_dir}`; `key_status` contains booleans/status only.
- `project.save {id?,name,description,style_id?,aspect_ratio?,preferences?,archived?,budget_microusd?}`; `project.duplicate {id}`; `project.list {}`
- `style.save {id?,name,blocks:{medium,lines,palette,light,texture},preview_prompt?}` → versioned style; `style.list {}`; `style.duplicate {id}`
- `character.save {id?,project_id,name,description,features,clothing,colors,binding:[],reference_ids:[],approved:bool}`; `character.list {project_id?}`
- `asset.import {project_id,name,description?,data_base64}` validates bytes, no arbitrary paths; `asset.list {project_id,query?,character_id?,style_id?,favorite?,status?,date?,offset?,limit?}` → `{items,total}`; `asset.update {id,description?,tags?:[],favorite?,comment?,status?}`; `asset.read {id,thumbnail?:bool}` → `{mime,data_base64}`; `asset.reveal {id}` (desktop only)
- `prompt.preview` / `cost.estimate` / `job.submit`: `{project_id,description,style_id?,character_id?,reference_ids:[],parent_id?,provider:'test'|'gemini'|'openai',model,aspect_ratio:'1:1',resolution:'1K',quality?:'low'|'medium'|'high'|'xhigh'|'max',count:1,blocks:{goal,scene,character,view,pose,expression,gaze,mouth,composition,output,avoid},variants?:[{pose,expression,view,mouth}],idempotency_key,unknown_price_approved?:bool}`. preview → `{prompt}`; estimate → `{per_image_microusd,total_microusd,currency:'USD',price_version,warnings:[]}`; submit → `{jobs:[records]}`. At most 32 explicit variants; `count` at most 8. Each job is one request and one image. No hidden cartesian product.
- `job.list {project_id?}` → `[]`; `job.get {id}`; `job.cancel {id}`; `cost.project {project_id}` → `{calculated_microusd,estimated_microusd,reserved_microusd,unknown_count,currency}`
- `settings.save {theme:'system'|'light'|'dark',mode:'simple'|'expert',export_dir?,naming_template?,job_limit_microusd?,project_limit_microusd?,warning_microusd?,concurrency:1,automation_approved:bool,rest_enabled:bool,mcp_enabled:bool}`; `secret.set {provider?:'gemini'|'openai'|'anthropic',key}`; `secret.delete {provider?}`; `tokens.create {name?,recipient?,purpose?,scope?:rest|mcp|both}` → `{token,access,notice}` once; `tokens.list {}`; `tokens.update {id,name?,recipient?,purpose?}`; `tokens.revoke {id?}` (secrets and tokens are handled by the main transport)
- `export.create {project_id,asset_ids:[],format:'original'|'png'|'jpeg'|'webp'|'tiff'|'html'|'pdf'|'pptx'|'zip'|'sprite',title?,description?,include_metadata:false,width?,height?,fit:'contain'|'cover'|'fill'|'preserve',background:'#ffffff',layout:'single'|'contact',cell_width?,cell_height?,columns?,padding?,naming_template?}` → `{id,path,files:[],...}`; `export.read {id}` → `{mime,data_base64}`; `project.restore {data_base64}` → project

## Exports Rust boundary

`pub fn create(root:&Path, project:&Value, assets:&[Value], options:&Value)->Result<Value>`; assets have `id` and `path` relative to root (e.g. `assets/id.png`), `description,width,height,format` plus metadata. Output goes under `root/exports` into a random exclusive export subdirectory; all formats are generated in pure Rust without runtime requirements. Returns id/path/files.

`pub fn read_export(root:&Path,id:&str)->Result<Value>`

`pub fn restore(root:&Path,bytes:&[u8])->Result<Value>` safely stages files under root and returns the validated package `{project,assets,styles,characters,jobs?}`. The core remaps IDs and paths in a transaction. The ZIP manifest is versioned.

The core routes export operations through these functions and runs them with blocking-code safety. Key API: `crate::secrets::provider_key()->Result<String>`, `key_status()->Value`, `set_provider_key(&str)->Result<()>`, `delete_provider_key()->Result<()>`.

## Frontend bridge

In Tauri: `invoke('rpc',{op,args})`. For browser testing, the fallback is POST `/rpc` with JSON `{op,args}`; the bearer token is supplied only by the test environment through browser memory or an injected global, never localStorage; no mock data. Native import uses an HTML file input → Base64; export downloads use `export.read` bytes; the desktop shows the export path. `main` may intercept secret, token and reveal operations.

## Provider keys and capabilities

`state.key_status.providers.gemini`, `.openai` and `.anthropic` contain only `{configured,status,store}`. The legacy `key_status.configured` refers to Gemini. For `secret.set/delete`, an omitted provider means Gemini for compatibility; invalid types or unknown providers are rejected. These operations are available only to the local GUI/CLI, never REST/MCP.

`configured` (boolean) only says that an entry was found; it confirms neither provider validity nor later read permission. `status` is `configured`, `missing` or `unavailable`. With `unavailable`, `configured` is false, but the status must not be shown as a definitely missing entry. The status query never requests key text.

Models and supported parameters are read from `state.models`. OpenAI supports the dated GPT-Image-2.5 models listed there, the three app formats 1:1/3:2/2:3 and quality levels. For OpenAI, `quality` is explicitly set to `medium` in the snapshot if missing; Gemini and the test provider reject the field. An API key is not a ChatGPT login.

## Style previews

- `style.preview.prepare {id}` returns `{project,style,description,blocks,aspect_ratio}` and creates an ordinary neutral project "Stilvorschauen" (style previews) if needed. No job and no provider call. Non-archived preview projects are reused.
- Build the generation request from the result: `project_id: project.id`, `style_id: style.id`, `description`, `blocks`, `aspect_ratio: "1:1"`, the chosen provider/model/quality and `count: 1`. Use the existing `prompt.preview`, `cost.estimate`, `job.submit` and `job.get`.
- `asset.list {project_id,style_id,offset?,limit?}` filters by the historical style snapshot before paginating.
- `style.preview.set {id,asset_id}` sets the explicitly chosen generated asset. Its style snapshot must match the style ID and contain a valid historical version. `asset_id: null` removes only the link. A missing `asset_id` is an error.
- Style fields: `preview_prompt` (backward-compatible default for old records), `preview_asset_id`, `preview_style_version`, `preview_provider`. Text and style changes stay versioned. Setting or removing a preview image does not change the style definition.

An older preview is kept after a style change and recognizable as older through the version comparison. Duplicated styles start without a preview image. The API performs no hidden recalculations. REST/MCP use the same operations and approval rules as the GUI.

## Price revision and text jobs

`pricing.catalog`, `pricing.refresh`, `assistant.models`, `assistant.estimate`, `assistant.submit`: full payloads and behavior in [API.md](API.md#pricing-and-the-claude-prompt-assistant), machine-readable in [openapi.json](openapi.json). Image and text submit support `expected_price_version`; `state.pricing_revision` is an opaque catalog hash for UI recalculation. Text jobs have `kind: prompt_assist` and the finished output in `result_text`; older jobs without `kind` count as image jobs. A ZIP with metadata contains the text job, inputs, system text, result and price snapshot. Sharing without metadata contains no jobs and no assistant texts. Restore sets jobs to `restored` and never starts them again.

## Access tokens

The local-only `tokens.*` operations and the MCP environment contract are described in [API.md](API.md#local-access-management). Lists return only metadata and stable access IDs, never hashes or the bearer token again. Revocation applies per access to REST and/or MCP according to the scope set at creation.

## Series with one local approval

- `series.submit {project_id,description,idempotency_key,items:[{description,provider,model,style_id?,character_id?,reference_ids?,aspect_ratio?,resolution?,quality?,blocks?,count?,variants?},…]}`. `items` uses the image parameters of `job.submit`, without its own `project_id`/`idempotency_key`; 1–32 expanded images. Description at most 1000 UTF-8 bytes. Unknown fields, including `origin`, are rejected.
- `series.list {project_id?}`, `series.get {id}`: persistent requests including `origin`, `count`, `estimated_microusd`, `currency`, `items[].snapshot`, `job_ids`, `jobs`, `job_status_counts`. `status`: `pending|approved|rejected|cancelled`; `approved` refers to the approval, processing progress is in the job statuses.
- `series.cancel {id}`: the whole request, best effort for running requests.
- Local only: `series.approve {id}`, `series.reject {id}`. Approving twice never starts duplicates. No approval after rejection or cancellation.

OpenAPI and the MCP tool schemas contain the four public series operations. Price or budget errors never produce a partially started series. The database version stays 1; new record type `series`, additive job fields `series_id`, `series_index`, `origin`. A project ZIP transfers assets and existing job history, but no active series approvals or access tokens; an imported project therefore cannot unlock automation.

### Structured prompts

`description` is the concrete individual instruction. `blocks.scene` describes the shared scene; `blocks.character` supplements the library character selected via `character_id` or describes further characters. `blocks.view` is the view. The core composes `[Gesamte Szene]` (scene), optionally `[Charakter]` (character) and `[Einzelanweisung]` (individual instruction) in this order. Project goal, style and character definition are added on the server side; clients should not duplicate them in `description`. Empty optional sections are omitted. Get a preview via `prompt.preview`; snapshot and final prompt are frozen with the job. Old stored prompts are never recomposed.
