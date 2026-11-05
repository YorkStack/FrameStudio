# Architecture and security model

*Deutsch: [de/ARCHITECTURE.md](de/ARCHITECTURE.md)*

FrameStudio is a local Tauri 2 app: React renders the interface, Rust owns data, provider access, budget rules and exports. The installed program contains SQLite and all export encoders. No Node, Python, Docker or Office installation is needed to run it. CLI, REST and MCP run from the same executable and use `Core::call`; there is no separate web data store.

## Data

Default location: the OS application-data directory (`ProjectDirs`; macOS `Library/Application Support`, Windows `LocalAppData`). `--data-dir` explicitly selects another local store. The interface shows the active path. Originals and thumbnails are stored as files next to SQLite. UUIDs are stable. SQLite transactions protect metadata and reservations; file-based atomic publishing prevents partially visible images. Unconfirmed provider requests are not blindly repeated on restart. A cross-process worker lock prevents parallel consumers of the same store.

The OS account is the local trust boundary. A process with full access to the same user account can modify files; FrameStudio does not provide multi-user isolation. Do not share a data directory over network file systems.

## Secrets and access

Gemini, OpenAI and Anthropic keys are stored under separate fixed accounts (`gemini-api-key`, `openai-api-key`, `anthropic-api-key`) in the service `com.yorkstack.framestudio`, if persistent storage is chosen explicitly. Alternatively, an explicitly activated in-process session store is available. macOS uses native Keychain Services, Windows the Credential Manager (`keyring`). There is no plaintext fallback. The GUI writes keys and only receives status back. Keys never go into project manifests, SQLite, browser storage or URLs. REST and MCP use independent random tokens that can be revoked individually; only SHA-256 digests and access metadata are stored, in a separate `access.sqlite3`. The plaintext token is shown once at creation. The access database is never part of a project backup.

The status display never reads a password. macOS queries attributes only; Windows uses `get_attributes`. The status distinguishes `configured`, `missing` and `unavailable`. Presence does not confirm validity at the provider. Other platforms without a verified metadata path report `unavailable`.

On macOS, `SecKeychainSetUserInteractionAllowed(false)` disables optional authentication dialogs for the whole FrameStudio process before the first keychain call. In addition, all `SecItem*` queries contain `kSecUseAuthenticationUIFail`; that query option alone does not cover all classic keychain access paths. The setting is process-local and changes neither other apps nor persistent keychain ACLs. Read, write/replace and delete use native `SecItem*` operations without reading the password first. Existing accounts are kept. Locked or unauthorized entries return errors; unlocking and app-specific authorization are done manually by the user when needed. No automatic ACL extension, deletion or migration.

A shared access gate prevents parallel operations. The GUI receives an error after at most 2 s for status and 8 s for read/write/delete. A thread still blocked in the OS is not killed: the gate stays occupied until it actually finishes. After a timeout the outcome of a write is unknown; there is no automatic retry. An exclusive file lock at the local cache path `credential-access/operation.lock` also prevents concurrent access from new FrameStudio processes; it contains no secrets. Older builds that are still running do not know this lock and must be quit completely. Errors and progress are shown directly at the key field, and a synchronous UI gate prevents double clicks.

REST must be enabled deliberately, binds only to `127.0.0.1:43120` and checks the exact host including port. `Origin` headers are rejected; there is no CORS allowance. All REST routes, including the schema, require a token. MCP runs as a stdio process started by the local client, also requires activation and offers no key, token or settings management. External jobs cannot grant themselves spending approval. The trusted local CLI administration mode `--rpc` has the same rights as the GUI; never feed untrusted input into it.

## Inputs and files

Imports pass image bytes, never freely readable file paths or URLs. Type, size and pixel budget are checked. Export names are sanitized and never interpreted as shell commands. ZIP contents are checked for relative paths, symlinks, entry and total sizes and manifest version. Exports go into isolated new directories and never overwrite originals. React escapes text; HTML exports escape text and ship a restrictive CSP. Prompts, comments and provider responses are treated as data.

The Tauri webview has no shell or file-system plugins, no remote fonts or scripts and makes no provider calls from the browser. Only the app's own RPC command is reachable. The CSP restricts scripts to local app resources and forbids frames, objects and form posts. Provider access happens from Rust over TLS to fixed Google and OpenAI endpoints without redirects. OpenAI uses only `https://api.openai.com/v1/images/generations` and `/v1/images/edits`; there are no configurable third-party URLs. Switching providers sends that provider's own key.

## Costs

Amounts are integer micro-USD. An estimate is not an invoice. Price version, sources and usage are recorded on the job. Missing usage stays unknown. Project costs include requests even when no visible assets result. Local limits restrict starts in this installation, not the provider account as a whole. Unclear results are not retried automatically. Paid extras are switched off.

## Price sources and the optional text assistant

`pricing.rs` checks a fixed, limited set of public vendor sources. The persistent JSON document `records(kind=pricing,id=global)` extends schema 1 without migration; older stores use the checked bundled catalog until the first refresh. A separate process lock prevents lost updates; SQLite stores the finished catalog atomically. Jobs keep their own price snapshot.

`assistant.rs` encapsulates only Anthropic's text API. `core.rs` uses the same queue, worker lock, idempotency and budget transaction for image and text jobs. The additive JSON fields `kind` and `result_text` need no new table. Text suggestions remain data; neither response nor prompt is executed as a command. The UI applies results only on explicit request. The native key module gets its own Anthropic account and no new webview permission. New fixed network trust boundaries: `api.anthropic.com` for explicit text jobs and public vendor documentation sources for price checks. REST/MCP cannot supply a target URL or credentials for these.

## Named accesses and the access database

`tokens.rs` manages `access.sqlite3`, separate from the project database. Migration from an unversioned store to `user_version=1` runs under `BEGIN IMMEDIATE`: existing hashes and creation times are kept; stable IDs and metadata are added. Fields: id, name, recipient, purpose, scope, last_used_at, revoked_at. A unique index protects IDs. Revocation is a permanent status and does not delete the access record. Storage uses FULL synchronization, file mode 0600 on Unix and rejects symlinks for the database and its sidecar files.

The local bearer token is built from 256 random UUID bits minus format bits; only its SHA-256 digest is stored. Name, recipient and purpose are plain text and SQL is parameterized. The full token is handed to the local interface once, never to lists, project packages, logs or localStorage. MCP takes the bearer only from the local host's `FRAMESTUDIO_MCP_TOKEN`, never from a URL or CLI option. It is checked at start and on every tool operation; no worker starts after a rejected login. The shared file system remains a trust boundary: the same OS user keeps local administration rights.

`.gitignore` also excludes the runtime databases `framestudio.sqlite3*` and `access.sqlite3*` including journals, in case a data directory inside the repository is chosen by mistake. Provider keys stay in the OS credential store regardless; neither the access database nor project backups contain provider keys.

## Explicit session keys

`secrets/session.rs` holds at most one key per provider under a mutex, in process memory only. `provider_key_for` checks it first; on a hit, the OS store is not accessed. This applies to Gemini, OpenAI and the Claude text assistant alike. Replacing or removing overwrites the stored string via `Zeroizing<String>`; the copy returned to provider adapters is protected the same way. Copies inside IPC, the webview and HTTP libraries as well as OS swap or crash dumps cannot be fully controlled; no guarantee is claimed that every copy in memory is erased. The application never persists the key and never returns it.

`secret.session.set`, `.remove` and `.status` are local administration operations and not part of the REST/MCP allowlist. Setting and removing do not touch the keychain. The status contains provider and a boolean only; the general state adds `status=session`, `configured=true`, `store=Session memory`. No database migration. After the process ends there is no recovery path. A separate MCP or headless process gets no copy. Requests already running keep their own short-lived copy; removing a key does not cancel a provider request. Budget and automation rules are unchanged.

## 0.2.0: prompt templates and libraries

Additive fields in existing JSON records, no new SQLite table or schema migration: project `prompt_template`, `deleted_at`; character `portrait_asset_id`; style `preview_hidden`; AI job `result_sections` and snapshot parameter `target`. ZIP backups with metadata keep these fields; ID remapping covers character portraits. Historical prompts stay unchanged.

Trash operations run in transactions and refuse deletion while jobs or series approvals are open. Data and costs are kept. Generation from deleted projects is rejected. Generated style candidates and automatic character portraits are derived from asset snapshots; imported photos are never automatic portraits. Libraries refresh their state every two seconds.

Eleven pre-generated default style previews ship as static PNG thumbnails in the frontend bundle, with a manifest containing style version, visual blocks and SHA-256. They create no assets, requests or costs in new user projects and are never sent to a provider as references automatically.

The SBOM is generated deterministically from `package-lock.json` and `Cargo.lock` during the npm build and embedded into the core via `include_str!`. No new app dependencies, Tauri permissions, provider targets or CSP allowances. Python 3.11+ is a build requirement only; CI uses the same npm build step. Cargo license data, the full dependency graph and OS frameworks are not yet fully part of this inventory SBOM.

## Further reading

- [Security design](SECURITY-DESIGN.md) — series approvals, key access and public distribution
- [Operation contract](CONTRACT.md) and [REST/MCP](API.md)
- [Exports and project packages](EXPORTS.md)
