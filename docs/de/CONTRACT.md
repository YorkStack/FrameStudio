# FrameStudio-Operationsvertrag (v1)

*English: [../CONTRACT.md](../CONTRACT.md)*

Ein Rust-Crate `framestudio` unter `src-tauri` mit optionalem Feature `desktop`. Dieselben Operationen bedienen GUI, REST und MCP; Transportdetails in [API.md](API.md), maschinenlesbares Schema in [openapi.json](../openapi.json).

Die Abschnitte zu Rust- und Frontend-Schnittstellen sind bewusst auf Englisch belassen, weil sie Code-Signaturen beschreiben.

## Core Rust boundary
`pub struct Core` is Clone + Send + Sync, holds root PathBuf. `pub fn open(root: PathBuf) -> anyhow::Result<Self>` migrates DB. `pub async fn call(&self, op: &str, args: serde_json::Value, automated: bool) -> anyhow::Result<Value>` dispatches all methods. `pub async fn work_once(&self) -> Result<bool>` atomically claims one job, process lock prevents multiple workers. Main runs worker loop. `pub fn root(&self) -> &Path`. Core checks automated spend approval, durable budgets/idempotency.
All JSON snake_case. Objects use string UUID `id`. Money integer `*_microusd`; null means unknown. Calls return records directly or arrays (list calls). Errors are German human readable.

## Operations and payload
- state {} -> {projects,styles,characters,settings,models,assistant_models,pricing_revision,key_status,data_dir}; key_status boolean/status only.
- project.save {id?,name,description,style_id?,aspect_ratio?,preferences?,archived?,budget_microusd?}; project.duplicate {id}; project.list {}
- style.save {id?,name,blocks:{medium,lines,palette,light,texture},preview_prompt?} -> versioned style; style.list {}; style.duplicate {id}
- character.save {id?,project_id,name,description,features,clothing,colors,binding:[],reference_ids:[],approved:bool}; character.list {project_id?}
- asset.import {project_id,name,description?,data_base64} validates bytes, no arbitrary paths; asset.list {project_id,query?,character_id?,style_id?,favorite?,status?,date?,offset?,limit?} -> {items,total}; asset.update {id,description?,tags?:[],favorite?,comment?,status?}; asset.read {id,thumbnail?:bool} -> {mime,data_base64}; asset.reveal {id} main desktop only
- prompt.preview / cost.estimate / job.submit: {project_id,description,style_id?,character_id?,reference_ids:[],parent_id?,provider:'test'|'gemini'|'openai',model,aspect_ratio:'1:1',resolution:'1K',quality?:'low'|'medium'|'high'|'xhigh'|'max',count:1,blocks:{goal,scene,character,view,pose,expression,gaze,mouth,composition,output,avoid},variants?:[{pose,expression,view,mouth}],idempotency_key,unknown_price_approved?:bool}. preview -> {prompt}; estimate -> {per_image_microusd,total_microusd,currency:'USD',price_version,warnings:[]}; submit -> {jobs:[records]}. Max 32 explicit variants; max count 8. Each job one request/image. No hidden cartesian product.
- job.list {project_id?} -> []; job.get {id}; job.cancel {id}; cost.project {project_id} -> {calculated_microusd,estimated_microusd,reserved_microusd,unknown_count,currency}
- settings.save {theme:'system'|'light'|'dark',mode:'simple'|'expert',export_dir?,naming_template?,job_limit_microusd?,project_limit_microusd?,warning_microusd?,concurrency:1,automation_approved:bool,rest_enabled:bool,mcp_enabled:bool}; secret.set {provider?:'gemini'|'openai'|'anthropic',key}; secret.delete {provider?:'gemini'|'openai'|'anthropic'}; tokens.create {name?,recipient?,purpose?,scope?:rest|mcp|both} -> {token,access,notice} once; tokens.list {}; tokens.update {id,name?,recipient?,purpose?}; tokens.revoke {id?} (main transport handles secrets/tokens)
- export.create {project_id,asset_ids:[],format:'original'|'png'|'jpeg'|'webp'|'tiff'|'html'|'pdf'|'pptx'|'zip'|'sprite',title?,description?,include_metadata:false,width?,height?,fit:'contain'|'cover'|'fill'|'preserve',background:'#ffffff',layout:'single'|'contact',cell_width?,cell_height?,columns?,padding?,naming_template?} -> {id,path,files:[],...}; export.read {id} -> {mime,data_base64}; project.restore {data_base64} -> project

## Exports Rust boundary
`pub fn create(root:&Path, project:&Value, assets:&[Value], options:&Value)->Result<Value>`; assets have `id, path` relative to root (e.g assets/id.png), `description,width,height,format` plus metadata. Output under root/exports, random exclusive export subdirectory; all formats generated in pure Rust, no runtime requirement. Return id/path/files.
`pub fn read_export(root:&Path,id:&str)->Result<Value>`
`pub fn restore(root:&Path,bytes:&[u8])->Result<Value>` safely stages files under root, returns the validated package `{project,assets,styles,characters,jobs?}`. Core remaps IDs and paths transactionally. ZIP manifest versioned.
Core routes export operations through these functions, runs with blocking code safety. Main key API `crate::secrets::provider_key()->Result<String>`, `key_status()->Value`, `set_provider_key(&str)->Result<()>`, `delete_provider_key()->Result<()>`.

## Frontend bridge
`invoke('rpc',{op,args})` in Tauri. Browser testing fallback POST `/rpc` JSON `{op,args}`, Authorization Bearer only test environment supplied via browser memory/global injected by test, never localStorage; no mock data. native import with HTML input file -> base64; download export via export.read bytes; desktop export path displayed. Main may intercept secret/token/reveal operations.

## Provider-Schlüssel und Fähigkeiten

`state.key_status.providers.gemini` , `.openai` und `.anthropic` enthalten ausschließlich `{configured,status,store}`. Das bisherige `key_status.configured` bleibt Gemini zugeordnet. Bei `secret.set/delete` bedeutet ein ausgelassener Provider aus Kompatibilitätsgründen Gemini; ungültige Typen oder unbekannte Provider werden abgewiesen. Diese Operationen sind nur für die lokale GUI/CLI zugänglich, niemals REST/MCP.

Modelle und unterstützte Parameter werden aus `state.models` gelesen. OpenAI unterstützt die dort angegebenen datierten GPT-Image-2.5-Modelle, die drei App-Formate 1:1/3:2/2:3 und Qualitätsstufen. `quality` wird für OpenAI im Snapshot explizit auf `medium` gesetzt, wenn es fehlt; Gemini und Testprovider weisen das Feld ab. Ein API-Schlüssel ist kein ChatGPT-Login.

## Stilvorschauen

- `style.preview.prepare {id}` liefert `{project,style,description,blocks,aspect_ratio}` und legt bei Bedarf ein gewöhnliches, neutrales Projekt `Stilvorschauen` an. Kein Job und kein Provideraufruf. Nicht archivierte Vorschauprojekte werden wiederverwendet.
- Aus dem Ergebnis den Generierungsauftrag bauen: `project_id: project.id`, `style_id: style.id`, `description`, `blocks`, `aspect_ratio: "1:1"`, ausgewählter Provider/Modell/Qualität sowie `count: 1`. Bestehende `prompt.preview`, `cost.estimate`, `job.submit` und `job.get` nutzen.
- `asset.list {project_id,style_id,offset?,limit?}` filtert nach dem historischen Stilsnapshot, bevor paginiert wird.
- `style.preview.set {id,asset_id}` setzt das ausdrücklich gewählte generierte Asset. Dessen Stilsnapshot muss zur Stil-ID passen und eine gültige historische Version enthalten. `asset_id: null` entfernt nur die Zuordnung. Fehlendes `asset_id` ist ein Fehler.
- Stilfelder: `preview_prompt` (bei alten Datensätzen rückwärtskompatibler Standard), `preview_asset_id`, `preview_style_version`, `preview_provider`. Änderungen an Text/Stil bleiben versioniert. Das Setzen/Entfernen eines Vorschaubilds ändert die Stildefinition nicht.

Eine ältere Vorschau bleibt nach einer Stiländerung erhalten und wird über den Versionsvergleich als älter erkennbar. Duplizierte Stile beginnen ohne Vorschaubild. Die API führt keine heimlichen Neuberechnungen aus. REST/MCP nutzen dieselben Operationen und Freigaberegeln wie die GUI.

### Schlüsselstatus ohne Geheimniszugriff

`state.key_status.providers.gemini` , `.openai` und `.anthropic` enthalten `configured` (Boolean), `status` (`configured`, `missing` oder `unavailable`) und `store`. `configured` sagt nur aus, dass ein Eintrag gefunden wurde; es bestätigt weder Provider-Gültigkeit noch spätere Freigabe zum Lesen. Bei `unavailable` ist `configured` false, der Status darf aber nicht als sicher fehlender Eintrag dargestellt werden. Die Statusabfrage fordert keinen Schlüsseltext an.


## Preisrevision und Textjobs

`pricing.catalog`, `pricing.refresh`, `assistant.models`, `assistant.estimate`, `assistant.submit`: vollständige Payloads und Verhalten siehe [API.md](API.md#preise-und-claude-prompt-assistent), maschinenlesbar in [openapi.json](../openapi.json). Bild-/Text-Submit unterstützt `expected_price_version`; `state.pricing_revision` ist ein undurchsichtiger Kataloghash für UI-Neuberechnung. Textjobs haben `kind: prompt_assist`, fertige Ausgabe `result_text`; ältere Jobs ohne kind gelten als Bildjobs. ZIP mit Metadaten erhält Textjob, Eingaben, Systemtext, Ergebnis und Preissnapshot. Teilen ohne Metadaten enthält keine Jobs und keine Assistententexte. Wiederherstellung setzt Jobs auf `restored` und startet sie niemals erneut.


## Zugriffstokens

Die nur lokal zugänglichen `tokens.*`-Operationen und der MCP-Umgebungsvertrag stehen in [API.md](API.md#lokale-zugangsverwaltung). Listen liefern ausschließlich Metadaten und stabile Zugangs-IDs; niemals Hashes oder erneut den Bearer-Token. Widerruf wirkt pro Zugang auf REST und/oder MCP entsprechend dem bei Erstellung festgelegten Bereich.

## Serien mit einmaliger lokaler Freigabe

- `series.submit {project_id,description,idempotency_key,items:[{description,provider,model,style_id?,character_id?,reference_ids?,aspect_ratio?,resolution?,quality?,blocks?,count?,variants?},…]}`. `items` nutzt die Bildparameter von `job.submit`, ohne eigenes `project_id`/`idempotency_key`; 1–32 expandierte Bilder. Beschreibung maximal 1000 UTF-8-Bytes. Unbekannte Felder einschließlich `origin` werden abgewiesen.
- `series.list {project_id?}`, `series.get {id}`: persistente Aufträge inklusive `origin`, `count`, `estimated_microusd`, `currency`, `items[].snapshot`, `job_ids`, `jobs`, `job_status_counts`. `status`: `pending|approved|rejected|cancelled`; `approved` bezeichnet die Freigabe, der Verarbeitungsfortschritt steht in den Jobstatuswerten.
- `series.cancel {id}`: gesamter Auftrag, bestmöglich für laufende Requests.
- Nur lokal: `series.approve {id}`, `series.reject {id}`. Mehrfaches Freigeben startet keine Duplikate. Nach Ablehnung/Abbruch kein erneutes Freigeben.

OpenAPI und MCP-Toolschemas enthalten die vier öffentlichen Serienoperationen. Preis-/Budgetfehler erzeugen keine teilweise gestartete Serie. Die Datenbankversion bleibt 1; neuer Recordtyp `series`, additive Jobfelder `series_id`, `series_index`, `origin`. Projekt-ZIP überträgt Assets und bestehende Jobhistorie, jedoch keine aktiven Serienfreigaben oder Zugriffstokens; ein importiertes Projekt kann dadurch keine Automation freischalten.

### Gegliederte Prompts

`description` ist die konkrete Einzelanweisung. `blocks.scene` beschreibt die gemeinsame Szene; `blocks.character` ergänzt die über `character_id` ausgewählte Bibliotheksfigur oder beschreibt weitere Figuren. `blocks.view` ist die Ansicht. Der Kern komponiert `[Gesamte Szene]`, optional `[Charakter]` und `[Einzelanweisung]` in dieser Reihenfolge. Projektziel, Stil und Charakterdefinition werden serverseitig einbezogen; Clients sollen sie nicht zusätzlich in `description` duplizieren. Leere optionale Abschnitte entfallen. Vorschau über `prompt.preview` abrufen; Snapshot und endgültiger Prompt werden beim Auftrag eingefroren. Alte gespeicherte Prompts werden nicht neu zusammengesetzt.
