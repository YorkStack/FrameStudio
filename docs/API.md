# REST and MCP

*Deutsch: [de/API.md](de/API.md)*

Enable REST or MCP in the settings. Automated paid jobs additionally require explicit spending approval. Everything is off by default. Jobs for the test provider only create local test images.

## Headless

macOS binary: `FrameStudio.app/Contents/MacOS/framestudio`.

```sh
"/path/to/FrameStudio.app/Contents/MacOS/framestudio" --serve
```

Starts REST and the persistent worker without a window. If you use a non-default data store, pass the same `--data-dir /path/to/store` to GUI, CLI and MCP. Several processes may open the same store; the worker lock serializes requests. A running GUI enables REST itself. If the port is already taken, do not start a second REST instance; MCP can run alongside.

`--rpc` reads one local JSON request from stdin and prints its JSON result. This mode is trusted administration, for example to configure settings before a headless start. Never pass provider keys as command-line arguments or write them into scripts.

## REST

Under **Settings → REST, MCP & access**, create a named REST access, note recipient and purpose, copy the token (shown once) and store it safely with the client. `scripts/rest-example.py` prompts for it without echo and lists projects. The server accepts only host `127.0.0.1:43120`, a bearer token and no `Origin` header. It is deliberately not a browser web service. Individual revocation in the settings is checked on every request; other accesses stay valid.

- GET `/v1/projects`
- GET `/v1/jobs/{id}`
- GET `/openapi.json`
- POST `/v1/operations/{operation}` with a JSON argument object
- POST `/rpc` with `{ "op": "project.list", "args": {} }`

All operations and payloads are listed in [CONTRACT.md](CONTRACT.md); they are identical for GUI, REST and MCP. The schema is also available offline via `framestudio --openapi`. Errors: HTTP 400 with `error`; unauthorized requests: 401. Image and export bytes are returned as `data_base64` and `mime`, not only as an inaccessible path.

Typical flow: `project.save` → `style.list` → `character.save` → `asset.import` → `prompt.preview` → `cost.estimate` → `job.submit` (unique `idempotency_key`) → `job.get`/`job.list` → `asset.list`/`asset.read` → `export.create`/`export.read`. Reusing the unchanged idempotency key on a retry prevents a second local start. A changed request needs a new key. This is not an exactly-once guarantee from the provider.

## MCP over stdio

Uses the official Rust SDK `rmcp`, not a home-grown JSON-RPC replacement. Configuration for a local MCP host (adjust the path to your installation):

```json
{"mcpServers":{"framestudio":{"command":"/path/to/FrameStudio.app/Contents/MacOS/framestudio","args":["--mcp"]}}}
```

The local host must pass `FRAMESTUDIO_MCP_TOKEN` to the MCP child process. Create an access with scope **MCP** or **REST & MCP** in FrameStudio. The configuration above intentionally contains no secret; without the variable the start fails with a clear message. Do not use provider keys as local access tokens. The active OS session provides keychain access to provider keys separately. Tool names replace the dot in operation names with an underscore, e.g. `job_submit`. `asset_read` returns native MCP image content plus structured metadata, `export_read` the complete file bytes. Local executable SDK client:

```zsh
# Enter the token without echo; it does not appear on the command line or in shell history.
read -rs 'FRAMESTUDIO_MCP_TOKEN?MCP access token: '
print
export FRAMESTUDIO_MCP_TOKEN
cargo run --manifest-path src-tauri/Cargo.toml --no-default-features --example mcp_client -- /path/to/framestudio
unset FRAMESTUDIO_MCP_TOKEN
```

For your own MCP host, use its secret or environment-variable management. A host started from Finder does not inherit variables from a terminal window. Never put real tokens into Git, example JSON or shell scripts. The bundled SDK client inherits the variable; the server checks the scope at start and revocation on every tool call and on `tools/list`. A rejected start does not start a worker.

A hosted model cannot reach localhost or stdio on this computer by itself. A local agent of your own starts the MCP process, calls tools and, if needed, passes results to its model API. That agent decides explicitly which local content leaves the machine. FrameStudio does not open a tunnel.

## Persistent export jobs

For longer exports, call `export.submit` with the same arguments as `export.create`. The response contains a persistent export job ID; `export.get {id}` returns `queued`, `running`, `completed` or `failed`. After `completed`, `result.id` is the ID for `export.read`. The GUI uses this flow. `export.create` remains available as a synchronous convenience operation. `export.preview` shows the file names sanitized by the core in advance. `--worker` runs only the consumer, without HTTP port or window. Export jobs are local and cause no provider costs.

## OpenAI alongside Gemini

Store the keys separately in the app settings. `cost.estimate`, `prompt.preview` and `job.submit` accept `provider: "openai"`. Read the supported model IDs and capabilities from `state.models` first. Example arguments (replace the project ID):

```json
{"project_id":"PROJECT_UUID","description":"A technical line drawing of a workshop","provider":"openai","model":"gpt-image-2.5-sunburst-2026-09-08","aspect_ratio":"3:2","resolution":"1K","quality":"medium","count":1,"reference_ids":[],"idempotency_key":"workshop-image-001"}
```

Here `1K` means 1024×1024, 1536×1024 or 1024×1536 depending on the aspect ratio. References use existing asset IDs from the same project. Automated OpenAI jobs need the same prior spending approval and atomic budget reservation as Gemini. Never pass keys in a request. The arguments above do not start anything on their own; that requires `job.submit`.

## Style previews via REST/MCP

`style.preview.prepare {id}` prepares a style and its ordinary preview project. Then check the returned fields with `style_id: style.id` and `project_id: project.id` via `prompt.preview` and `cost.estimate`, and only then start with `job.submit`. When the job is done, call `asset.list` with `style_id` and choose the desired result explicitly with `style.preview.set {id,asset_id}`. MCP names: `style_preview_prepare` and `style_preview_set`. Preparing and assigning are free; generation is a normal budgeted provider job and, when automated, still requires local spending approval.

## Pricing and the Claude prompt assistant

- `pricing.catalog {}` returns providers, models, price versions, sources, check dates and image price examples clearly labeled as output share. Claude has `capability: prompt_assistance` and `no_image_generation: true`.
- `pricing.refresh {provider?: "gemini"|"openai"|"anthropic"}` checks public sources; without a choice, all three. No model request, no keys and no user-supplied URL. A provider status of `error` means the previous prices are kept. `updated` confirms a checked price source, not a successful generation.
- `assistant.models {}` returns supported text models. `assistant.estimate {project_id,text,instruction?,provider?,model?}` returns micro-USD, warnings and `price_version`. Anthropic is the default; the free test assistant must be selected explicitly with `provider: "test"`.
- `assistant.submit` takes the same fields plus `idempotency_key` and optionally `expected_price_version`. Response `{jobs,reused}`. `job.get`/`job.cancel` work unchanged; the finished text is in `result_text`, the job kind is `kind: prompt_assist`. No assets or follow-up jobs are created.
- `cost.project` contains, besides the totals, `by_kind.image_generation` and `by_kind.prompt_assist`. Both count against the same atomic start limits. Automated Anthropic jobs need prior spending approval even if `provider` was omitted.

MCP names replace dots with underscores, e.g. `assistant_estimate` and `pricing_refresh`. Example for a **local, free** text test (replace the project ID):

```json
{"op":"assistant.submit","args":{"project_id":"PROJECT_UUID","text":"A retro robot in a workshop.","instruction":"Describe the composition in more detail.","provider":"test","idempotency_key":"prompt-test-001"}}
```

For image and text jobs, send the estimate's `price_version` as `expected_price_version`. The core rejects a price that changed in the meantime before reserving. Repeated identical requests keep the same idempotency key. Keys are stored only locally in the settings, never in a REST/MCP payload.

## Local access management

Trusted GUI/CLI administration only, not callable via REST/MCP:

| Operation | Payload / result |
|---|---|
| `tokens.list` | `{}` → list of metadata, never token or hash |
| `tokens.create` | `{name,recipient,purpose,scope}` → `{token,access,notice}`; `scope`: `rest`, `mcp`, `both`; token shown once |
| `tokens.update` | `{id,name?,recipient?,purpose?}` → metadata; scope and creation time cannot be changed |
| `tokens.revoke` | `{id}` → `{revoked:true,count}`; revokes only this access; history is kept |

Metadata: `id`, `name`, `recipient`, `purpose`, `scope`, `created_at`, `last_used_at`, `revoked_at`, `active`. Times come from the local system; new values are RFC 3339/UTC. Older SQLite creation times are UTC as well. "Last used" means successful access authentication, not a successfully completed job. The recipient is a self-maintained label, not a verified user account.

Compatibility: existing REST tokens remain valid and appear as "Existing API access". They can be labeled afterwards. `tokens.create {}` still works for local scripts (REST), and `tokens.revoke {}` still revokes all active accesses. The GUI uses individual revocation only. **MCP requires a matching token from this version on**; older token-less MCP configurations must add the host environment described above.

Revocation blocks new operations but does not stop an already accepted job or recall data already retrieved. Cancel jobs separately if needed. Token rights cover the permitted project operations of the chosen transport; paid automation still needs its separate local approval and budgets. The same OS user can administer the local CLI: tokens manage client access, they are not isolation against the owner of the local files.

After an update, stop all old GUI, headless and MCP instances once running jobs are finished, and start them with the new binary. Older versions that are still running do not know the new token management, so the new checks do not apply to them. The app does not start or stop other user processes automatically.

## One approval for a complete series

Use **`series.submit` / `series_submit`** for series. The client submits description, project, a stable idempotency key and the concrete list of all images together. Response: series ID, status `pending`, authenticated origin, image count and total estimate in micro-USD. No job is processed and no budget reserved yet. In the running GUI, open **Series approvals**, review the summary and image list and choose **Approve entire series**. The approval covers all frozen images; no per-image confirmation is needed. It does not grant general spending approval for later jobs.

Afterwards, `series.get {"id":"…"}` / `series_get` returns `job_ids`, all jobs and `job_status_counts`; `asset.list/read` return the results. `series.cancel` stops waiting images and requests best-effort cancellation of running ones. `series.approve/reject` are local administration operations only, not REST/MCP tools. Without a GUI a request stays pending; the local owner can approve it after review via trusted `--rpc`. A hosted model API cannot grant this local approval itself.

Origin: transport REST/MCP plus ID, name, recipient and purpose of the token that actually authenticated. The label documents the access, not a verified person or the specific program behind a shared token. The client cannot inject an `origin` field. Changing a token label does not change historical origins. Revocation blocks new interface calls; reject or cancel already submitted or approved series separately if needed.

Limits: at most 32 images after expanding `count`/`variants`, at most 100 pending approvals. One project per series; style, character, references, prompt, provider and model may differ per image. Subjects, references and prompt blocks stay frozen. Before local approval, the user may choose provider, model, quality and resolution for the entire series. Unsupported aspect ratios or references block that choice instead of silently adapting content. The submitted state is kept under `submitted_items`, the approved snapshots under `items`. Content changes need a new request and a new idempotency key. Price changes after the displayed preview block the start until it is checked again. On approval, the overall job limit and the current project budget are checked and reserved in the same SQLite transaction as all job starts. Costs are estimates, not a guaranteed provider cap. A failure is not retried automatically; the remaining approved images continue. Retrying a single image successfully is a new request.

**Compatibility:** `job.submit` remains available for direct jobs and, for paid automation, still needs the existing general local approval. For the per-series approval described here, use `series.submit`; the general automation setting can stay off. Session keys exist only in their own process. If a series is approved while a session key is active, only a worker with an active session key for that provider claims these jobs; an MCP or headless process without that key skips them. After a restart, the images stay waiting until the session key is entered again. The approval is kept; the key is never transferred between processes.

Executable examples (local test provider by default): put your project ID into `examples/series-request.json`. For real images, set provider and model explicitly in the JSON; local series approval is still required.

```sh
python3 scripts/rest-series.py examples/series-request.json
python3 scripts/rest-series.py --status SERIES-ID
# Pass the MCP token to the local host securely, as shown above:
cargo run --manifest-path src-tauri/Cargo.toml --locked --no-default-features --example mcp_series_client -- /path/to/framestudio examples/series-request.json
```

Never write provider keys or access tokens into the JSON file. Idempotency applies per transport and access: the same key with identical content returns the same series, even after a restart or a lost response. Resubmitting a rejected series does not reopen it.

## Compare models before approving a series

`series.quote {id, selection?: {provider, model, resolution, quality?}}` (MCP `series_quote`) computes a complete new cost preview without provider call, job or reservation. Without `selection`, the model choice of the submitted images is kept; with a selection, it applies to all images. Response: `items` with unchanged prompts/references, `estimated_microusd`, `count`, `currency`, `quote_id` and the general `job_limit_microusd`. Use quality levels and resolutions from `state.models`. Only existing capabilities are allowed; formats are never changed automatically.

The GUI shows models greyed out with estimates even when the key is missing. There is currently one stored key per provider plus an optional session key that takes priority; multiple accounts for the same provider are not supported. Key status does not confirm provider permission; local read access is checked before approval.

Local only: `series.approve {id, selection?, expected_quote}` recomputes the same preview inside the start transaction. A different `quote_id` prevents the start. A provider change without `expected_quote` is rejected. The original request and authenticated origin are kept; resubmitting with the original idempotency key does not create a second request.

If the series exceeds the general start limit, the local user can confirm an additional checkbox for this series only. The approval then contains `series_limit_microusd` with exactly the checked total. This changes no global setting, does not lift the project budget and cannot be granted via REST/MCP. Without this explicit choice, the general start limit applies unchanged. The total is reserved atomically for all images; costs remain estimates, not a guaranteed invoice.

Local key diagnosis: `secret.check {provider, source?: "active"|"keychain"}`. The default `active` uses an existing session key before the OS store. `keychain` checks only the OS store without removing a session key. Returns status only: no plaintext, no provider call. This operation stays blocked for REST/MCP.

## Export only the images of a series

`asset.list` additionally accepts `series_id` together with `project_id`. The response contains only saved results of the series jobs in their order, with `total`, `offset` and `limit` (at most 200 per page). Imported references and other series are excluded; a series from another project is rejected. Fetch all pages and pass the ordered `items[].id` as `asset_ids` to `export.submit`. Do not start an export for an empty result: an empty export selection still means "entire project". `export.get` returns the status; `export.read` returns the file, or a ZIP as Base64 for several files. No regeneration and no provider costs.

## Additions in 0.2.0

`project.prompt.save`: `{id, prompt_template: {scene, character, instruction}}` stores the three editor defaults; only these text fields are allowed (8000 UTF-8 bytes each). `project.save` accepts the same template. For automated generation, still pass the desired `blocks.scene`, `blocks.character` and `description` explicitly; the template does not replace request parameters.

`assistant.estimate` / `assistant.submit` accept `target: scene | character | instruction | all` (default: instruction). After validation, `all` returns `result_sections` with the three fields; an unstructured or invalid result is treated as an error and never applied automatically. Model answers remain paid text jobs; approvals and budgets apply unchanged. `character.save` accepts `portrait_asset_id`: a generated asset from the same project, independent of `reference_ids`. An empty string enables automatic portrait selection.

The project trash and the file-system SBOM export are local administration functions and are not exposed as automated REST/MCP tools.
